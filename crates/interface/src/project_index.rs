//! Project Index — persistent cross-process project registry and discovery.
//!
//! Extracted from `agent_mode.rs` (SEC-S1 God Object refactoring).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const PROJECT_INDEX_VERSION: u32 = 1;
pub(crate) const PROJECT_INDEX_MAX_ENTRIES: usize = 256;
pub(crate) const PROJECT_INDEX_MAX_SESSION_IDS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedProjectRecord {
    pub(crate) project_id: String,
    pub(crate) project_root: PathBuf,
    pub(crate) working_dir: PathBuf,
    pub(crate) worktree: PathBuf,
    pub(crate) recent_session_ids: Vec<String>,
    pub(crate) last_seen_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedProjectIndex {
    pub(crate) version: u32,
    pub(crate) projects: Vec<PersistedProjectRecord>,
}

impl Default for PersistedProjectIndex {
    fn default() -> Self {
        Self {
            version: PROJECT_INDEX_VERSION,
            projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectIndexStore {
    pub(crate) path: PathBuf,
    pub(crate) index: PersistedProjectIndex,
}

impl ProjectIndexStore {
    pub(crate) fn load_default() -> Self {
        let path = project_index_file_path();
        let index = load_project_index(path.as_path()).unwrap_or_default();
        Self { path, index }
    }

    pub(crate) fn known_project_roots(&self, limit: usize) -> Vec<PathBuf> {
        let mut entries = self.index.projects.clone();
        entries.sort_by(|left, right| right.last_seen_unix_ms.cmp(&left.last_seen_unix_ms));
        entries
            .into_iter()
            .filter_map(|entry| canonicalize_existing_dir(entry.project_root.as_path()))
            .take(limit.max(1))
            .collect()
    }

    pub(crate) fn known_project_ids(&self) -> Vec<String> {
        let mut ids = self
            .index
            .projects
            .iter()
            .map(|entry| entry.project_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    pub(crate) fn upsert(
        &mut self,
        identity: &ndc_core::ProjectIdentity,
        session_id: Option<&str>,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        let index = self.index.projects.iter().position(|entry| {
            entry.project_id == identity.project_id && entry.project_root == identity.project_root
        });
        let mut sessions = session_id
            .map(|value| vec![value.to_string()])
            .unwrap_or_default();
        if let Some(idx) = index {
            let entry = &mut self.index.projects[idx];
            if let Some(sid) = session_id {
                sessions.extend(
                    entry
                        .recent_session_ids
                        .iter()
                        .filter(|existing| existing.as_str() != sid)
                        .cloned(),
                );
            } else {
                sessions.extend(entry.recent_session_ids.iter().cloned());
            }
            sessions.truncate(PROJECT_INDEX_MAX_SESSION_IDS);
            entry.working_dir = identity.working_dir.clone();
            entry.worktree = identity.worktree.clone();
            entry.recent_session_ids = sessions;
            entry.last_seen_unix_ms = now;
        } else {
            self.index.projects.push(PersistedProjectRecord {
                project_id: identity.project_id.clone(),
                project_root: identity.project_root.clone(),
                working_dir: identity.working_dir.clone(),
                worktree: identity.worktree.clone(),
                recent_session_ids: sessions,
                last_seen_unix_ms: now,
            });
        }

        self.index
            .projects
            .sort_by(|left, right| right.last_seen_unix_ms.cmp(&left.last_seen_unix_ms));
        self.index.projects.truncate(PROJECT_INDEX_MAX_ENTRIES);
    }

    pub(crate) fn save(&self) -> io::Result<()> {
        save_project_index(self.path.as_path(), &self.index)
    }
}

pub(crate) fn project_index_file_path() -> PathBuf {
    if let Ok(value) = std::env::var("NDC_PROJECT_INDEX_FILE") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    ndc_core::ConfigLayer::User
        .path()
        .join("project_index.json")
}

fn load_project_index(path: &Path) -> Option<PersistedProjectIndex> {
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: PersistedProjectIndex = serde_json::from_str(raw.as_str()).ok()?;
    Some(parsed)
}

fn save_project_index(path: &Path, index: &PersistedProjectIndex) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(index).map_err(io::Error::other)?;
    std::fs::write(path, data)
}

// ---------------------------------------------------------------------------
// Project discovery helpers
// ---------------------------------------------------------------------------

pub(crate) fn build_project_scoped_session_id(project_id: &str) -> String {
    let short_project = project_id.chars().take(8).collect::<String>();
    format!("agent-{}-{}", short_project, ulid::Ulid::new())
}

pub(crate) fn canonicalize_existing_dir(path: &Path) -> Option<PathBuf> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    Some(std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

pub(crate) fn looks_like_project_root(path: &Path) -> bool {
    path.join(".git").exists()
        || path.join(".ndc").exists()
        || [
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "pom.xml",
            "Makefile",
        ]
        .iter()
        .any(|marker| path.join(marker).exists())
}

pub(crate) fn discover_project_directories(seed_dirs: &[PathBuf], limit: usize) -> Vec<PathBuf> {
    let cap = limit.max(1);
    let mut seen = BTreeSet::<PathBuf>::new();
    let mut candidates = Vec::<PathBuf>::new();
    fn push_unique(
        seen: &mut BTreeSet<PathBuf>,
        candidates: &mut Vec<PathBuf>,
        path: PathBuf,
    ) -> bool {
        if seen.insert(path.clone()) {
            candidates.push(path);
            true
        } else {
            false
        }
    }
    let mut canonical_seeds = Vec::<PathBuf>::new();
    let mut seed_seen = BTreeSet::<PathBuf>::new();
    for seed in seed_dirs {
        let Some(seed) = canonicalize_existing_dir(seed.as_path()) else {
            continue;
        };
        if seed_seen.insert(seed.clone()) {
            canonical_seeds.push(seed);
        }
    }

    // First pass: include seed project roots directly (ensures persisted projects are not starved).
    for seed in &canonical_seeds {
        if looks_like_project_root(seed.as_path()) {
            let inserted = push_unique(&mut seen, &mut candidates, seed.clone());
            if inserted && candidates.len() >= cap {
                return candidates;
            }
        }
    }

    // Second pass: expand parent/sibling/child directories.
    for seed in canonical_seeds {
        if let Some(parent) = seed.parent().and_then(canonicalize_existing_dir) {
            if looks_like_project_root(parent.as_path()) {
                let inserted = push_unique(&mut seen, &mut candidates, parent.clone());
                if inserted && candidates.len() >= cap {
                    return candidates;
                }
            }
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Some(path) = canonicalize_existing_dir(path.as_path()) else {
                        continue;
                    };
                    if looks_like_project_root(path.as_path()) {
                        let inserted = push_unique(&mut seen, &mut candidates, path);
                        if inserted && candidates.len() >= cap {
                            return candidates;
                        }
                    }
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(seed.as_path()) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(path) = canonicalize_existing_dir(path.as_path()) else {
                    continue;
                };
                if looks_like_project_root(path.as_path()) {
                    let inserted = push_unique(&mut seen, &mut candidates, path);
                    if inserted && candidates.len() >= cap {
                        return candidates;
                    }
                }
            }
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Serialize env-mutating tests.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn test_project_index_store_roundtrip() {
        let _guard = env_lock();
        let temp = TempDir::new().expect("temp dir");
        let index_path = temp.path().join("project_index.json");
        unsafe {
            std::env::set_var(
                "NDC_PROJECT_INDEX_FILE",
                index_path.to_string_lossy().to_string(),
            );
        }

        let project = temp.path().join("demo");
        std::fs::create_dir_all(project.as_path()).expect("create project dir");
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname=\"demo\"\nversion=\"0.1.0\"\n",
        )
        .expect("write marker");
        let identity = ndc_core::ProjectIdentity::detect(Some(project.clone()));

        let mut store = ProjectIndexStore::load_default();
        store.upsert(&identity, Some("agent-demo-session"));
        store.save().expect("save index");

        let reloaded = ProjectIndexStore::load_default();
        let ids = reloaded.known_project_ids();
        assert!(ids.contains(&identity.project_id));
        let roots = reloaded.known_project_roots(10);
        assert!(roots.contains(&identity.project_root));

        unsafe {
            std::env::remove_var("NDC_PROJECT_INDEX_FILE");
        }
    }
}
