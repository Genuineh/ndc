//! `AgentBackend` trait implementation for `AgentModeManager`.
//!
//! Bridges the gap between `ndc-tui::AgentBackend` (trait) and
//! `ndc-interface::AgentModeManager` (concrete type) so the TUI crate
//! never imports `ndc-interface` directly.

use std::path::PathBuf;

use async_trait::async_trait;
use ndc_core::{AgentExecutionEvent, AgentResponse, AgentSessionExecutionEvent, ModelInfo};
use tokio::sync::mpsc;

use ndc_tui::{
    AgentBackend, AgentStatus, ProjectCandidate, ProjectSwitchInfo, TuiPermissionRequest,
};

use crate::agent_mode::{AgentModeManager, handle_agent_command};
use crate::permission_engine::PermissionRequest;

#[async_trait]
impl AgentBackend for AgentModeManager {
    async fn status(&self) -> AgentStatus {
        let s = self.status().await;
        AgentStatus {
            enabled: s.enabled,
            agent_name: s.agent_name,
            provider: s.provider,
            model: s.model,
            session_id: s.session_id,
            project_id: s.project_id,
            project_root: s.project_root,
            worktree: s.worktree,
        }
    }

    async fn session_timeline(
        &self,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<AgentExecutionEvent>> {
        Ok(self.session_timeline(limit).await?)
    }

    async fn subscribe_execution_events(
        &self,
    ) -> anyhow::Result<(
        String,
        tokio::sync::broadcast::Receiver<AgentSessionExecutionEvent>,
    )> {
        Ok(self.subscribe_execution_events().await?)
    }

    async fn process_input(&self, input: &str) -> anyhow::Result<AgentResponse> {
        Ok(self.process_input(input).await?)
    }

    async fn switch_provider(&self, provider: &str, model: Option<&str>) -> anyhow::Result<()> {
        Ok(self.switch_provider(provider, model).await?)
    }

    async fn switch_model(&self, model: &str) -> anyhow::Result<()> {
        Ok(self.switch_model(model).await?)
    }

    async fn list_models(&self, provider: Option<&str>) -> anyhow::Result<Vec<ModelInfo>> {
        Ok(self.list_models(provider).await?)
    }

    async fn use_session(&self, id: &str, read_only: bool) -> anyhow::Result<String> {
        Ok(self.use_session(id, read_only).await?)
    }

    async fn resume_latest_project_session(&self) -> anyhow::Result<String> {
        Ok(self.resume_latest_project_session().await?)
    }

    async fn start_new_session(&self) -> anyhow::Result<String> {
        Ok(self.start_new_session().await?)
    }

    async fn list_project_session_ids(
        &self,
        prefix: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        Ok(self.list_project_session_ids(prefix, limit).await?)
    }

    async fn switch_project_context(&self, path: PathBuf) -> anyhow::Result<ProjectSwitchInfo> {
        let outcome = self.switch_project_context(path).await?;
        Ok(ProjectSwitchInfo {
            project_id: outcome.project_id,
            project_root: outcome.project_root,
            session_id: outcome.session_id,
            resumed_existing_session: outcome.resumed_existing_session,
        })
    }

    async fn discover_projects(&self, limit: usize) -> anyhow::Result<Vec<ProjectCandidate>> {
        let candidates = self.discover_projects(limit).await?;
        Ok(candidates
            .into_iter()
            .map(|c| ProjectCandidate {
                project_id: c.project_id,
                project_root: c.project_root,
            })
            .collect())
    }

    async fn handle_agent_command(&self, input: &str) -> anyhow::Result<()> {
        handle_agent_command(input, self).await?;
        Ok(())
    }

    async fn set_permission_channel(&self, tx: mpsc::Sender<TuiPermissionRequest>) {
        // Bridge: spawn a forwarding task that converts TuiPermissionRequest → PermissionRequest
        let (inner_tx, mut inner_rx) = mpsc::channel::<PermissionRequest>(16);

        tokio::spawn(async move {
            while let Some(req) = inner_rx.recv().await {
                let tui_req = TuiPermissionRequest {
                    description: req.description,
                    permission_key: req.permission_key,
                    response_tx: req.response_tx,
                };
                if tx.send(tui_req).await.is_err() {
                    break;
                }
            }
        });

        self.set_permission_channel(inner_tx).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_backend_is_object_safe() {
        // Verify AgentBackend can be used as trait object
        fn _assert_object_safe(_: &dyn AgentBackend) {}
    }
}
