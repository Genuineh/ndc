//! Scene — lightweight rendering hint derived from workflow stage + tool name.
//!
//! Scene classifies the current TUI context into one of six modes, each
//! providing a badge label and an accent color for the chat renderer.
//! This is a pure UI-layer helper (~50 lines); no LLM calls, no cross-crate deps.

use ratatui::style::Color;

/// Rendering-hint scene for the current TUI context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    /// Free-form conversation (default)
    Chat,
    /// Code / context analysis (discovery stage)
    Analyze,
    /// Planning / reasoning
    Plan,
    /// Code writing / editing
    Implement,
    /// Shell execution / debugging
    Debug,
    /// Test verification / review
    Review,
}

impl Scene {
    /// Short label for the title-bar badge.
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::Chat => "对话",
            Self::Analyze => "分析",
            Self::Plan => "规划",
            Self::Implement => "实现",
            Self::Debug => "调试",
            Self::Review => "验证",
        }
    }

    /// Accent color associated with this scene.
    pub fn accent_color(&self) -> Color {
        match self {
            Self::Chat => Color::Cyan,
            Self::Analyze => Color::Blue,
            Self::Plan => Color::Magenta,
            Self::Implement => Color::Green,
            Self::Debug => Color::Yellow,
            Self::Review => Color::LightGreen,
        }
    }
}

/// Classify the current context into a [`Scene`] based on the active workflow
/// stage and the most recent tool name.
///
/// The mapping is intentionally simple (pure pattern match, < 1 ms):
/// - `"load_context"` / `"compress"` / `"analysis"` → Analyze
/// - `"planning"`  → Plan
/// - `"executing"` + write/edit tool → Implement
/// - `"executing"` + shell tool      → Debug
/// - `"verifying"` / `"completing"` / `"reporting"` → Review
/// - everything else → Chat
pub fn classify_scene(workflow_stage: Option<&str>, tool_name: Option<&str>) -> Scene {
    match workflow_stage {
        Some("load_context") | Some("compress") | Some("analysis") => Scene::Analyze,
        Some("planning") => Scene::Plan,
        Some("executing") => match tool_name {
            Some(t) if is_write_tool(t) => Scene::Implement,
            Some(t) if is_shell_tool(t) => Scene::Debug,
            _ => Scene::Implement,
        },
        Some("verifying") | Some("completing") | Some("reporting") => Scene::Review,
        _ => Scene::Chat,
    }
}

/// Returns `true` for tool names that write or edit files.
fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write_file"
            | "edit_file"
            | "create_file"
            | "patch_file"
            | "replace_in_file"
            | "insert_code"
    )
}

/// Returns `true` for tool names that run shell commands.
fn is_shell_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "run_command" | "shell" | "exec" | "bash" | "terminal" | "run_shell"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_scene tests ──────────────────────────────────────

    #[test]
    fn test_no_stage_no_tool_is_chat() {
        assert_eq!(classify_scene(None, None), Scene::Chat);
    }

    #[test]
    fn test_unknown_stage_is_chat() {
        assert_eq!(classify_scene(Some("unknown"), None), Scene::Chat);
    }

    #[test]
    fn test_planning_stage() {
        assert_eq!(classify_scene(Some("planning"), None), Scene::Plan);
        assert_eq!(
            classify_scene(Some("planning"), Some("write_file")),
            Scene::Plan
        );
    }

    #[test]
    fn test_discovery_stage_maps_to_chat() {
        // Discovery was removed in P1-Workflow; old "discovery" stage falls to Chat
        assert_eq!(classify_scene(Some("discovery"), None), Scene::Chat);
    }

    #[test]
    fn test_executing_with_write_tool() {
        assert_eq!(
            classify_scene(Some("executing"), Some("write_file")),
            Scene::Implement
        );
        assert_eq!(
            classify_scene(Some("executing"), Some("edit_file")),
            Scene::Implement
        );
        assert_eq!(
            classify_scene(Some("executing"), Some("create_file")),
            Scene::Implement
        );
    }

    #[test]
    fn test_executing_with_shell_tool() {
        assert_eq!(
            classify_scene(Some("executing"), Some("run_command")),
            Scene::Debug
        );
        assert_eq!(
            classify_scene(Some("executing"), Some("shell")),
            Scene::Debug
        );
        assert_eq!(
            classify_scene(Some("executing"), Some("bash")),
            Scene::Debug
        );
    }

    #[test]
    fn test_executing_with_unknown_tool_defaults_implement() {
        assert_eq!(
            classify_scene(Some("executing"), Some("search_code")),
            Scene::Implement
        );
    }

    #[test]
    fn test_executing_no_tool_defaults_implement() {
        assert_eq!(classify_scene(Some("executing"), None), Scene::Implement);
    }

    #[test]
    fn test_verifying_stage() {
        assert_eq!(classify_scene(Some("verifying"), None), Scene::Review);
    }

    #[test]
    fn test_completing_stage() {
        assert_eq!(classify_scene(Some("completing"), None), Scene::Review);
    }

    // ── P1-Workflow new stage mappings ────────────────────────────

    #[test]
    fn test_load_context_stage() {
        assert_eq!(classify_scene(Some("load_context"), None), Scene::Analyze);
    }

    #[test]
    fn test_compress_stage() {
        assert_eq!(classify_scene(Some("compress"), None), Scene::Analyze);
    }

    #[test]
    fn test_analysis_stage() {
        assert_eq!(classify_scene(Some("analysis"), None), Scene::Analyze);
    }

    #[test]
    fn test_reporting_stage() {
        assert_eq!(classify_scene(Some("reporting"), None), Scene::Review);
    }

    #[test]
    fn test_discovery_stage_no_longer_valid() {
        // Discovery was removed in P1-Workflow; fallback to Chat
        assert_eq!(classify_scene(Some("discovery"), None), Scene::Chat);
    }

    // ── badge_label tests ─────────────────────────────────────────

    #[test]
    fn test_badge_labels_non_empty() {
        for scene in [
            Scene::Chat,
            Scene::Analyze,
            Scene::Plan,
            Scene::Implement,
            Scene::Debug,
            Scene::Review,
        ] {
            assert!(!scene.badge_label().is_empty());
        }
    }

    // ── accent_color tests ────────────────────────────────────────

    #[test]
    fn test_accent_colors_distinct() {
        let scenes = [
            Scene::Chat,
            Scene::Analyze,
            Scene::Plan,
            Scene::Implement,
            Scene::Debug,
            Scene::Review,
        ];
        let colors: Vec<Color> = scenes.iter().map(|s| s.accent_color()).collect();
        // Each color should be unique
        for (i, c) in colors.iter().enumerate() {
            for (j, d) in colors.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        c, d,
                        "Scene {:?} and {:?} share color",
                        scenes[i], scenes[j]
                    );
                }
            }
        }
    }
}
