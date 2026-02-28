//! AgentBackend trait — abstracts `AgentModeManager` for TUI consumption.
//!
//! This trait lives in `ndc-tui` so the TUI never depends on `ndc-interface`.
//! `ndc-interface` provides the concrete implementation via
//! `impl AgentBackend for AgentModeManager`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ndc_core::{AgentExecutionEvent, AgentResponse, AgentSessionExecutionEvent, ModelInfo};

// ── DTO types (TUI-owned, mapped from interface types) ──────────────

/// Lightweight agent status snapshot for the TUI title bar.
#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub enabled: bool,
    pub agent_name: String,
    pub provider: String,
    pub model: String,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<PathBuf>,
    pub worktree: Option<PathBuf>,
}

/// Result of switching project context.
#[derive(Debug, Clone)]
pub struct ProjectSwitchInfo {
    pub project_id: String,
    pub project_root: PathBuf,
    pub session_id: String,
    pub resumed_existing_session: bool,
}

/// A discovered project candidate.
#[derive(Debug, Clone)]
pub struct ProjectCandidate {
    pub project_id: String,
    pub project_root: PathBuf,
}

/// A permission request sent from the executor to the TUI for user confirmation.
#[derive(Debug)]
pub struct TuiPermissionRequest {
    pub description: String,
    pub permission_key: Option<String>,
    pub response_tx: tokio::sync::oneshot::Sender<bool>,
}

// ── Trait ────────────────────────────────────────────────────────────

/// Abstraction over agent operations that the TUI requires.
///
/// The concrete implementation lives in `ndc-interface` (`AgentModeManager`).
#[async_trait]
pub trait AgentBackend: Send + Sync {
    // --- Status ---
    async fn status(&self) -> AgentStatus;

    async fn session_timeline(
        &self,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<AgentExecutionEvent>>;

    async fn subscribe_execution_events(
        &self,
    ) -> anyhow::Result<(
        String,
        tokio::sync::broadcast::Receiver<AgentSessionExecutionEvent>,
    )>;

    // --- User input ---
    async fn process_input(&self, input: &str) -> anyhow::Result<AgentResponse>;

    // --- Provider / model ---
    async fn switch_provider(&self, provider: &str, model: Option<&str>) -> anyhow::Result<()>;

    async fn switch_model(&self, model: &str) -> anyhow::Result<()>;

    async fn list_models(&self, provider: Option<&str>) -> anyhow::Result<Vec<ModelInfo>>;

    // --- Session management ---
    async fn use_session(&self, id: &str, read_only: bool) -> anyhow::Result<String>;

    async fn resume_latest_project_session(&self) -> anyhow::Result<String>;

    async fn start_new_session(&self) -> anyhow::Result<String>;

    async fn list_project_session_ids(
        &self,
        prefix: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<String>>;

    // --- Project context ---
    async fn switch_project_context(&self, path: PathBuf) -> anyhow::Result<ProjectSwitchInfo>;

    async fn discover_projects(&self, limit: usize) -> anyhow::Result<Vec<ProjectCandidate>>;

    // --- Agent command passthrough ---
    async fn handle_agent_command(&self, input: &str) -> anyhow::Result<()>;

    // --- Permission channel ---
    async fn set_permission_channel(&self, tx: tokio::sync::mpsc::Sender<TuiPermissionRequest>);
}

/// Convenience type alias used throughout the TUI crate.
pub type DynAgentBackend = Arc<dyn AgentBackend>;
