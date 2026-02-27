//! Session storage — consolidated session, project index, and cursor management.
//!
//! Extracted from `orchestrator.rs` to reduce god-object complexity.
//! Holds sessions, project-session mapping, and latest-root cursor
//! under a single lock for race-condition safety.

use super::{AgentError, AgentExecutionEvent, AgentSession, ProjectIdentity};
use std::collections::HashMap;
use tracing::info;

/// Consolidated session storage — holds sessions, project index, and
/// latest-root cursor under a single lock to prevent race conditions.
pub(crate) struct SessionStore {
    sessions: HashMap<String, AgentSession>,
    project_sessions: HashMap<String, Vec<String>>,
    project_last_root: HashMap<String, String>,
}

impl SessionStore {
    pub(crate) fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            project_sessions: HashMap::new(),
            project_last_root: HashMap::new(),
        }
    }

    /// Index a session into project maps.
    fn index_session(&mut self, session: &AgentSession) {
        let entries = self
            .project_sessions
            .entry(session.project_id.clone())
            .or_insert_with(Vec::new);
        if !entries.iter().any(|id| id == &session.id) {
            entries.push(session.id.clone());
        }
        self.project_last_root
            .insert(session.project_id.clone(), session.id.clone());
    }

    /// Get an existing session or create a new one.
    /// Rejects cross-project continuation of existing sessions.
    pub(crate) fn get_or_create_session(
        &mut self,
        session_id: &str,
        working_dir: Option<std::path::PathBuf>,
    ) -> Result<AgentSession, AgentError> {
        let identity = ProjectIdentity::detect(working_dir);

        if let Some(existing) = self.sessions.get(session_id) {
            if existing.project_id != identity.project_id {
                return Err(AgentError::InvalidRequest(format!(
                    "session '{}' belongs to project '{}', current project is '{}'; cross-project session continuation is denied by default",
                    session_id, existing.project_id, identity.project_id
                )));
            }
            let session = existing.clone();
            self.index_session(&session);
            Ok(session)
        } else {
            let session = AgentSession::new_with_project_identity(session_id.to_string(), identity);
            self.sessions
                .insert(session_id.to_string(), session.clone());
            self.index_session(&session);
            info!("Created new session: {}", session_id);
            Ok(session)
        }
    }

    /// Save (insert or update) a session and update project index.
    pub(crate) fn save_session(&mut self, session: AgentSession) {
        self.sessions.insert(session.id.clone(), session.clone());
        self.index_session(&session);
    }

    /// Return a cloned session snapshot.
    pub(crate) fn session_snapshot(&self, session_id: &str) -> Option<AgentSession> {
        self.sessions.get(session_id).cloned()
    }

    /// Return latest session id for a project, prioritizing root sessions.
    pub(crate) fn latest_session_id_for_project(&self, project_id: &str) -> Option<String> {
        if let Some(session_id) = self.project_last_root.get(project_id) {
            return Some(session_id.clone());
        }
        self.sessions
            .values()
            .filter(|session| session.project_id == project_id)
            .max_by(|left, right| left.started_at.cmp(&right.started_at))
            .map(|session| session.id.clone())
    }

    /// Return project identity metadata for a session id.
    pub(crate) fn session_project_identity(&self, session_id: &str) -> Option<ProjectIdentity> {
        let session = self.sessions.get(session_id)?;
        Some(ProjectIdentity {
            project_id: session.project_id.clone(),
            project_root: session.project_root.clone(),
            working_dir: session.working_dir.clone(),
            worktree: session.worktree.clone(),
        })
    }

    /// Return known project ids tracked by the store.
    pub(crate) fn known_project_ids(&self) -> Vec<String> {
        let mut ids = self
            .sessions
            .values()
            .map(|session| session.project_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    /// Return session ids for a project ordered by latest activity.
    pub(crate) fn session_ids_for_project(
        &self,
        project_id: &str,
        limit: Option<usize>,
    ) -> Vec<String> {
        let mut entries = self
            .sessions
            .values()
            .filter(|session| session.project_id == project_id)
            .map(|session| (session.started_at, session.id.clone()))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| right.0.cmp(&left.0));
        if let Some(limit) = limit {
            entries.truncate(limit);
        }
        entries.into_iter().map(|(_, id)| id).collect()
    }

    /// Retrieve execution events for a session.
    pub(crate) fn get_session_execution_events(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentExecutionEvent>, AgentError> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            AgentError::SessionNotFound(format!("Session '{}' not found", session_id))
        })?;
        let events = &session.execution_events;
        let max = limit.unwrap_or(events.len());
        let start = events.len().saturating_sub(max);
        Ok(events[start..].to_vec())
    }

    /// Bulk import persisted sessions into the store.
    pub(crate) fn hydrate_sessions(&mut self, sessions: Vec<AgentSession>) {
        for session in sessions {
            self.sessions.insert(session.id.clone(), session.clone());
            self.index_session(&session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_agent::AgentSession;

    #[test]
    fn test_session_store_new_is_empty() {
        let store = SessionStore::new();
        assert!(store.sessions.is_empty());
        assert!(store.project_sessions.is_empty());
        assert!(store.project_last_root.is_empty());
    }

    #[test]
    fn test_save_and_retrieve_session() {
        let mut store = SessionStore::new();
        let mut session = AgentSession::new("sess-1".to_string());
        session.project_id = "proj-a".to_string();
        store.save_session(session.clone());

        let snapshot = store.session_snapshot("sess-1");
        assert!(snapshot.is_some());
        assert_eq!(snapshot.unwrap().project_id, "proj-a");

        assert!(store.session_snapshot("nonexistent").is_none());
    }

    #[test]
    fn test_known_project_ids_dedup_and_sorted() {
        let mut store = SessionStore::new();
        for (sid, pid) in [("s1", "proj-b"), ("s2", "proj-a"), ("s3", "proj-b")] {
            let mut s = AgentSession::new(sid.to_string());
            s.project_id = pid.to_string();
            store.save_session(s);
        }
        let ids = store.known_project_ids();
        assert_eq!(ids, vec!["proj-a", "proj-b"]);
    }

    #[test]
    fn test_session_ids_for_project_with_limit() {
        let mut store = SessionStore::new();
        for i in 0..5 {
            let mut s = AgentSession::new(format!("s-{}", i));
            s.project_id = "proj".to_string();
            store.save_session(s);
        }
        let all = store.session_ids_for_project("proj", None);
        assert_eq!(all.len(), 5);
        let limited = store.session_ids_for_project("proj", Some(2));
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn test_get_session_execution_events_not_found() {
        let store = SessionStore::new();
        let result = store.get_session_execution_events("nonexistent", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_hydrate_sessions_bulk_import() {
        let mut store = SessionStore::new();
        let sessions: Vec<AgentSession> = (0..3)
            .map(|i| {
                let mut s = AgentSession::new(format!("h-{}", i));
                s.project_id = "proj-hydrate".to_string();
                s
            })
            .collect();
        store.hydrate_sessions(sessions);
        assert_eq!(store.sessions.len(), 3);
        assert_eq!(store.known_project_ids(), vec!["proj-hydrate"]);
    }

    #[test]
    fn test_latest_session_id_uses_last_root() {
        let mut store = SessionStore::new();
        let mut s1 = AgentSession::new("s1".to_string());
        s1.project_id = "proj".to_string();
        store.save_session(s1);

        let mut s2 = AgentSession::new("s2".to_string());
        s2.project_id = "proj".to_string();
        store.save_session(s2);

        // last_root should point to s2 (latest saved)
        let latest = store.latest_session_id_for_project("proj");
        assert_eq!(latest, Some("s2".to_string()));

        // Re-save s1 updates last_root
        let mut s1_again = AgentSession::new("s1".to_string());
        s1_again.project_id = "proj".to_string();
        store.save_session(s1_again);
        let latest = store.latest_session_id_for_project("proj");
        assert_eq!(latest, Some("s1".to_string()));
    }

    #[test]
    fn test_session_project_identity() {
        let mut store = SessionStore::new();
        let mut s = AgentSession::new("s1".to_string());
        s.project_id = "proj-ident".to_string();
        store.save_session(s);

        let identity = store.session_project_identity("s1");
        assert!(identity.is_some());
        assert_eq!(identity.unwrap().project_id, "proj-ident");

        assert!(store.session_project_identity("missing").is_none());
    }

    #[tokio::test]
    async fn test_concurrent_save_index_consistency() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let store = Arc::new(Mutex::new(SessionStore::new()));
        let mut handles = Vec::new();

        for project_idx in 0..4 {
            let store = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                for session_idx in 0..10 {
                    let session_id = format!("s-{}-{}", project_idx, session_idx);
                    let mut session = AgentSession::new(session_id.clone());
                    session.project_id = format!("proj-{}", project_idx);

                    let mut guard = store.lock().await;
                    guard.save_session(session);
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let guard = store.lock().await;
        assert_eq!(guard.sessions.len(), 40);
        for i in 0..4 {
            let key = format!("proj-{}", i);
            let list = guard.project_sessions.get(&key).unwrap();
            assert_eq!(list.len(), 10, "project {} session list", i);
            assert!(guard.project_last_root.contains_key(&key));
        }
    }
}
