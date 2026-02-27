//! Runtime security gateway for tool execution.
//!
//! Enforces non-UI safety boundaries so CLI/daemon/repl share the same baseline:
//! - External directory boundary checks
//! - Shell risk-level gating
//! - Git high-risk operation gating

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};

use super::ToolError;
use super::bash_parsing::{BashDangerLevel, BashParser};
use std::cell::RefCell;

pub const PERMISSION_EXTERNAL_DIRECTORY: &str = "external_directory";
pub const PERMISSION_SHELL_HIGH_RISK: &str = "shell_high_risk";
pub const PERMISSION_SHELL_MEDIUM_RISK: &str = "shell_medium_risk";
pub const PERMISSION_GIT_COMMIT: &str = "git_commit";

tokio::task_local! {
    static SECURITY_OVERRIDE_PERMISSIONS: RefCell<HashSet<String>>;
}

#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock poisoned")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecurityAction {
    Allow,
    Ask,
    Deny,
}

impl SecurityAction {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "allow" => Some(Self::Allow),
            "ask" | "confirm" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

fn security_default_for_tests(default: SecurityAction) -> SecurityAction {
    if cfg!(test) {
        SecurityAction::Allow
    } else {
        default
    }
}

fn action_from_env(key: &str, default: SecurityAction) -> SecurityAction {
    std::env::var(key)
        .ok()
        .and_then(|value| SecurityAction::parse(&value))
        .unwrap_or_else(|| security_default_for_tests(default))
}

fn should_enforce_gateway() -> bool {
    std::env::var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY")
        .ok()
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true)
}

fn parse_security_override(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_string())
        .collect()
}

fn env_security_overrides() -> HashSet<String> {
    std::env::var("NDC_SECURITY_OVERRIDE_PERMISSIONS")
        .ok()
        .map(|raw| parse_security_override(raw.as_str()).into_iter().collect())
        .unwrap_or_default()
}

fn has_override(permission: &str) -> bool {
    SECURITY_OVERRIDE_PERMISSIONS
        .try_with(|overrides| overrides.borrow().contains(permission))
        .unwrap_or(false)
        || env_security_overrides().contains(permission)
}

fn ask_message(permission: &str, risk: &str, detail: &str) -> String {
    format!(
        "requires_confirmation permission={} risk={} {}",
        permission, risk, detail
    )
}

pub async fn with_security_overrides<F, T>(overrides: &[String], future: F) -> T
where
    F: Future<Output = T>,
{
    let mut merged = env_security_overrides();
    if let Ok(existing) = SECURITY_OVERRIDE_PERMISSIONS.try_with(|state| state.borrow().clone()) {
        merged.extend(existing);
    }
    merged.extend(
        overrides
            .iter()
            .map(|entry| entry.trim())
            .filter(|entry| !entry.is_empty())
            .map(|entry| entry.to_string()),
    );
    SECURITY_OVERRIDE_PERMISSIONS
        .scope(RefCell::new(merged), future)
        .await
}

pub fn extract_confirmation_permission(message: &str) -> Option<&str> {
    let prefix = "requires_confirmation permission=";
    let rest = message.strip_prefix(prefix)?;
    rest.split_whitespace()
        .next()
        .filter(|value| !value.is_empty())
}

fn project_root(working_dir_hint: Option<&Path>) -> PathBuf {
    let from_env = std::env::var("NDC_PROJECT_ROOT")
        .ok()
        .map(PathBuf::from)
        .or_else(|| working_dir_hint.map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    canonicalize_lossy(&from_env)
}

/// Normalize a path by resolving `.` and `..` components purely logically
/// (no filesystem access). This is used as a safe fallback when `canonicalize`
/// fails, preventing `..` traversal from fooling `Path::starts_with`.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = parts.last() {
                    parts.pop();
                } else if !matches!(parts.last(), Some(Component::RootDir)) {
                    // Only keep `..` if we haven't hit root; discard at root boundary
                    parts.push(component);
                }
            }
            Component::CurDir => {} // skip `.`
            _ => parts.push(component),
        }
    }
    parts.iter().collect()
}

fn canonicalize_lossy(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }

    if let Some(parent) = path.parent()
        && let Ok(parent_canonical) = std::fs::canonicalize(parent)
    {
        if let Some(name) = path.file_name() {
            return parent_canonical.join(name);
        }
        return parent_canonical;
    }

    // Fallback: normalize logically to prevent `..` traversal bypass
    normalize_path(path)
}

fn resolve_absolute(path: &Path, working_dir: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        return canonicalize_lossy(path);
    }

    let base = working_dir
        .map(canonicalize_lossy)
        .or_else(|| std::env::current_dir().ok().map(|p| canonicalize_lossy(&p)))
        .unwrap_or_else(|| PathBuf::from("."));
    canonicalize_lossy(&base.join(path))
}

/// Enforce project boundary for file-system like paths.
///
/// Returns `PermissionDenied` when target path is outside project root and policy is not `allow`.
pub fn enforce_path_boundary(
    path: &Path,
    working_dir: Option<&Path>,
    operation_label: &str,
) -> Result<(), ToolError> {
    if !should_enforce_gateway() {
        return Ok(());
    }

    let project_root = project_root(working_dir);
    let resolved = resolve_absolute(path, working_dir);
    if resolved.starts_with(&project_root) {
        return Ok(());
    }

    let action = action_from_env(
        "NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION",
        SecurityAction::Ask,
    );
    if matches!(action, SecurityAction::Ask) && has_override(PERMISSION_EXTERNAL_DIRECTORY) {
        return Ok(());
    }

    match action {
        SecurityAction::Allow => Ok(()),
        SecurityAction::Ask => Err(ToolError::PermissionDenied(ask_message(
            PERMISSION_EXTERNAL_DIRECTORY,
            "high",
            format!(
                "external_directory requires confirmation: {} -> {} is outside project root {}; set NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION=allow to override",
                operation_label,
                resolved.display(),
                project_root.display()
            )
            .as_str(),
        ))),
        SecurityAction::Deny => Err(ToolError::PermissionDenied(format!(
            "external_directory denied: {} -> {} is outside project root {}",
            operation_label,
            resolved.display(),
            project_root.display()
        ))),
    }
}

/// Enforce shell command risk gating + external directory boundary.
pub fn enforce_shell_command(
    command: &str,
    args: &[String],
    working_dir: Option<&Path>,
) -> Result<(), ToolError> {
    if !should_enforce_gateway() {
        return Ok(());
    }

    let mut full = command.to_string();
    if !args.is_empty() {
        full.push(' ');
        full.push_str(args.join(" ").as_str());
    }
    let parsed = BashParser::new()
        .parse(full.as_str())
        .map_err(ToolError::ExecutionFailed)?;

    let (permission_name, risk_action) = match parsed.danger_level {
        BashDangerLevel::Critical => ("", SecurityAction::Deny),
        BashDangerLevel::High => (PERMISSION_SHELL_HIGH_RISK, SecurityAction::Ask),
        BashDangerLevel::Medium => (
            PERMISSION_SHELL_MEDIUM_RISK,
            action_from_env("NDC_SECURITY_MEDIUM_RISK_ACTION", SecurityAction::Ask),
        ),
        BashDangerLevel::Low | BashDangerLevel::Safe => ("", SecurityAction::Allow),
    };

    if !permission_name.is_empty()
        && matches!(risk_action, SecurityAction::Ask)
        && has_override(permission_name)
    {
        // Approval override is per-call and only bypasses `ask`, never `deny`.
    } else {
        match risk_action {
            SecurityAction::Allow => {}
            SecurityAction::Ask => {
                let risk = match parsed.danger_level {
                    BashDangerLevel::High => "high",
                    BashDangerLevel::Medium => "medium",
                    _ => "unknown",
                };
                return Err(ToolError::PermissionDenied(ask_message(
                    permission_name,
                    risk,
                    format!(
                        "shell command requires confirmation (risk={:?}): {}",
                        parsed.danger_level, parsed.command
                    )
                    .as_str(),
                )));
            }
            SecurityAction::Deny => {
                return Err(ToolError::PermissionDenied(format!(
                    "shell command denied (risk={:?}): {}",
                    parsed.danger_level, parsed.command
                )));
            }
        }
    }

    for op in &parsed.file_operations {
        enforce_path_boundary(
            op.path.as_path(),
            working_dir,
            format!("shell:{}:{:?}", command, op.operation_type).as_str(),
        )?;
    }

    Ok(())
}

/// Enforce high-risk git operations.
pub fn enforce_git_operation(operation: &str) -> Result<(), ToolError> {
    if !should_enforce_gateway() {
        return Ok(());
    }

    if operation.eq_ignore_ascii_case("commit") {
        let action = action_from_env("NDC_SECURITY_GIT_COMMIT_ACTION", SecurityAction::Ask);
        if matches!(action, SecurityAction::Ask) && has_override(PERMISSION_GIT_COMMIT) {
            return Ok(());
        }

        return match action {
            SecurityAction::Allow => Ok(()),
            SecurityAction::Ask => Err(ToolError::PermissionDenied(ask_message(
                PERMISSION_GIT_COMMIT,
                "high",
                "git commit requires confirmation; set NDC_SECURITY_GIT_COMMIT_ACTION=allow to override",
            ))),
            SecurityAction::Deny => Err(ToolError::PermissionDenied(
                "git commit denied by policy".to_string(),
            )),
        };
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enforce_path_boundary_denies_external_when_policy_deny() {
        let _guard = test_env_lock();
        unsafe {
            std::env::set_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY", "1");
        }
        unsafe {
            std::env::set_var("NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION", "deny");
        }
        let root = std::env::temp_dir().join("ndc-security-root");
        let _ = std::fs::create_dir_all(&root);
        unsafe {
            std::env::set_var("NDC_PROJECT_ROOT", root.to_string_lossy().to_string());
        }
        let outside = std::env::temp_dir()
            .join("ndc-security-outside")
            .join("x.txt");
        let result = enforce_path_boundary(outside.as_path(), None, "write");
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
        unsafe {
            std::env::remove_var("NDC_PROJECT_ROOT");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
    }

    #[test]
    fn test_enforce_shell_command_denies_critical() {
        let _guard = test_env_lock();
        unsafe {
            std::env::set_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY", "1");
        }
        let args = vec!["-rf".to_string(), "/".to_string()];
        let result = enforce_shell_command("rm", &args, None);
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
    }

    #[test]
    fn test_enforce_git_operation_ask_by_default() {
        let _guard = test_env_lock();
        unsafe {
            std::env::set_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY", "1");
        }
        unsafe {
            std::env::set_var("NDC_SECURITY_GIT_COMMIT_ACTION", "ask");
        }
        let result = enforce_git_operation("commit");
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
        unsafe {
            std::env::remove_var("NDC_SECURITY_GIT_COMMIT_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
    }

    #[tokio::test]
    async fn test_with_security_overrides_allows_external_directory_once() {
        let _guard = test_env_lock();
        unsafe {
            std::env::set_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY", "1");
        }
        unsafe {
            std::env::set_var("NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION", "ask");
        }
        let root = std::env::temp_dir().join("ndc-security-root-once");
        let _ = std::fs::create_dir_all(&root);
        unsafe {
            std::env::set_var("NDC_PROJECT_ROOT", root.to_string_lossy().to_string());
        }
        let outside = std::env::temp_dir()
            .join("ndc-security-outside-once")
            .join("x.txt");

        let denied = enforce_path_boundary(outside.as_path(), None, "write");
        assert!(matches!(denied, Err(ToolError::PermissionDenied(_))));

        let override_once = vec![PERMISSION_EXTERNAL_DIRECTORY.to_string()];
        let allowed = with_security_overrides(override_once.as_slice(), async {
            enforce_path_boundary(outside.as_path(), None, "write")
        })
        .await;
        assert!(allowed.is_ok());

        unsafe {
            std::env::remove_var("NDC_PROJECT_ROOT");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
    }

    #[test]
    fn test_extract_confirmation_permission() {
        let message = "requires_confirmation permission=git_commit risk=high git commit requires confirmation";
        assert_eq!(extract_confirmation_permission(message), Some("git_commit"));
        assert_eq!(extract_confirmation_permission("plain denied"), None);
    }

    #[test]
    fn test_gateway_enforced_by_default_without_env_override() {
        let _guard = test_env_lock();
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
        assert!(should_enforce_gateway());
    }

    #[test]
    fn test_security_defaults_relaxed_in_tests_without_env_override() {
        let _guard = test_env_lock();
        unsafe {
            std::env::remove_var("NDC_SECURITY_MEDIUM_RISK_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_GIT_COMMIT_ACTION");
        }
        // NOTE: test builds intentionally relax policy defaults to allow
        // unless env vars explicitly request stricter behavior.
        assert_eq!(
            action_from_env("NDC_SECURITY_MEDIUM_RISK_ACTION", SecurityAction::Ask),
            SecurityAction::Allow
        );
        assert_eq!(
            action_from_env(
                "NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION",
                SecurityAction::Ask
            ),
            SecurityAction::Allow
        );
        assert_eq!(
            action_from_env("NDC_SECURITY_GIT_COMMIT_ACTION", SecurityAction::Ask),
            SecurityAction::Allow
        );
    }

    #[test]
    fn test_env_security_overrides_parses_csv_values() {
        let _guard = test_env_lock();
        unsafe {
            std::env::set_var(
                "NDC_SECURITY_OVERRIDE_PERMISSIONS",
                "external_directory, git_commit , shell_high_risk",
            );
        }
        let parsed = env_security_overrides();
        assert!(parsed.contains(PERMISSION_EXTERNAL_DIRECTORY));
        assert!(parsed.contains(PERMISSION_GIT_COMMIT));
        assert!(parsed.contains(PERMISSION_SHELL_HIGH_RISK));
        unsafe {
            std::env::remove_var("NDC_SECURITY_OVERRIDE_PERMISSIONS");
        }
    }

    #[test]
    fn test_normalize_path_removes_dotdot() {
        use std::path::Path;
        assert_eq!(
            normalize_path(Path::new("/a/b/../../etc/passwd")),
            PathBuf::from("/etc/passwd")
        );
        assert_eq!(
            normalize_path(Path::new("/project/src/../../../outside")),
            PathBuf::from("/outside")
        );
        assert_eq!(
            normalize_path(Path::new("/a/b/./c")),
            PathBuf::from("/a/b/c")
        );
        assert_eq!(normalize_path(Path::new("/a/b/c")), PathBuf::from("/a/b/c"));
    }

    #[test]
    fn test_dotdot_traversal_blocked_by_boundary_check() {
        let _guard = test_env_lock();
        unsafe {
            std::env::set_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY", "1");
        }
        unsafe {
            std::env::set_var("NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION", "deny");
        }
        // Use a non-existent deep path so canonicalize falls back to normalize
        let fake_root = PathBuf::from("/nonexistent/ndc-project-root-xyz");
        unsafe {
            std::env::set_var("NDC_PROJECT_ROOT", fake_root.to_string_lossy().to_string());
        }
        // Path that uses .. to escape: /nonexistent/ndc-project-root-xyz/../../etc/passwd
        // Without normalize_path, starts_with would incorrectly pass
        let attack_path = fake_root.join("../../etc/passwd");
        let result = enforce_path_boundary(attack_path.as_path(), None, "read");
        assert!(
            matches!(result, Err(ToolError::PermissionDenied(_))),
            "Path with .. traversal must be denied, but got: {result:?}"
        );
        unsafe {
            std::env::remove_var("NDC_PROJECT_ROOT");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_EXTERNAL_DIRECTORY_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
    }
}
