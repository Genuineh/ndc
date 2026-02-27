//! Session Archive — persistent cross-process session store.
//!
//! Extracted from `agent_mode.rs` (SEC-S1 God Object refactoring).

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const SESSION_ARCHIVE_VERSION: u32 = 1;
pub(crate) const SESSION_ARCHIVE_MAX_ENTRIES: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedSessionRecord {
    pub(crate) session: ndc_core::AgentSession,
    pub(crate) last_seen_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedSessionArchive {
    pub(crate) version: u32,
    pub(crate) sessions: Vec<PersistedSessionRecord>,
}

impl Default for PersistedSessionArchive {
    fn default() -> Self {
        Self {
            version: SESSION_ARCHIVE_VERSION,
            sessions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SessionArchiveStore {
    pub(crate) path: PathBuf,
    pub(crate) archive: PersistedSessionArchive,
}

impl SessionArchiveStore {
    pub(crate) fn load_default() -> Self {
        let path = session_archive_file_path();
        let archive = load_session_archive(path.as_path()).unwrap_or_default();
        Self { path, archive }
    }

    pub(crate) fn all_sessions(&self) -> Vec<ndc_core::AgentSession> {
        self.archive
            .sessions
            .iter()
            .map(|record| record.session.clone())
            .collect()
    }

    pub(crate) fn upsert(&mut self, session: &ndc_core::AgentSession) {
        let now = chrono::Utc::now().timestamp_millis();
        if let Some(idx) = self
            .archive
            .sessions
            .iter()
            .position(|record| record.session.id == session.id)
        {
            self.archive.sessions[idx].session = session.clone();
            self.archive.sessions[idx].last_seen_unix_ms = now;
        } else {
            self.archive.sessions.push(PersistedSessionRecord {
                session: session.clone(),
                last_seen_unix_ms: now,
            });
        }
        self.archive
            .sessions
            .sort_by(|left, right| right.last_seen_unix_ms.cmp(&left.last_seen_unix_ms));
        self.archive.sessions.truncate(SESSION_ARCHIVE_MAX_ENTRIES);
    }

    pub(crate) fn save(&self) -> io::Result<()> {
        save_session_archive(self.path.as_path(), &self.archive)
    }
}

pub(crate) fn session_archive_file_path() -> PathBuf {
    if let Ok(value) = std::env::var("NDC_SESSION_ARCHIVE_FILE") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    ndc_core::ConfigLayer::User
        .path()
        .join("session_archive.json")
}

fn load_session_archive(path: &Path) -> Option<PersistedSessionArchive> {
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: PersistedSessionArchive = serde_json::from_str(raw.as_str()).ok()?;
    Some(parsed)
}

fn save_session_archive(path: &Path, archive: &PersistedSessionArchive) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(archive).map_err(io::Error::other)?;
    std::fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_core::{AgentExecutionEvent, AgentExecutionEventKind};
    use tempfile::TempDir;

    // Serialize env-mutating tests.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn test_session_archive_store_roundtrip() {
        let _guard = env_lock();
        let temp = TempDir::new().expect("temp dir");
        let archive_path = temp.path().join("session_archive.json");
        unsafe {
            std::env::set_var(
                "NDC_SESSION_ARCHIVE_FILE",
                archive_path.to_string_lossy().to_string(),
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

        let mut session = ndc_core::AgentSession::new_with_project_identity(
            "agent-demo-session".to_string(),
            identity,
        );
        session.add_execution_event(AgentExecutionEvent {
            kind: AgentExecutionEventKind::Text,
            timestamp: chrono::Utc::now(),
            message: "persisted timeline event".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: Some(7),
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        });

        let mut store = SessionArchiveStore::load_default();
        store.upsert(&session);
        store.save().expect("save archive");

        let reloaded = SessionArchiveStore::load_default();
        let sessions = reloaded.all_sessions();
        let restored = sessions
            .iter()
            .find(|entry| entry.id == "agent-demo-session")
            .expect("restored session");
        assert_eq!(restored.project_id, session.project_id);
        assert_eq!(restored.execution_events.len(), 1);
        assert_eq!(
            restored.execution_events[0].message,
            "persisted timeline event"
        );

        unsafe {
            std::env::remove_var("NDC_SESSION_ARCHIVE_FILE");
        }
    }
}
