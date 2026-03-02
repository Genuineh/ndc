//! NDC TUI — Terminal User Interface for NDC.
//!
//! This crate provides the ratatui-based interactive session UI.
//! It depends on `ndc-core` for domain types and defines the
//! `AgentBackend` trait that `ndc-interface` implements.

pub mod agent_backend;
mod app;
mod chat_renderer;
mod commands;
mod event_renderer;
mod input_handler;
mod layout_manager;
pub mod scene;
pub mod todo_panel;
#[cfg(test)]
pub(crate) mod test_helpers;

// Re-export the trait and DTOs as the primary public API
pub use agent_backend::{
    AgentBackend, AgentStatus, DynAgentBackend, ProjectCandidate, ProjectSwitchInfo,
    TodoItem, TodoState, TuiPermissionRequest,
};

// Re-export TUI entry point and visualization state
pub use app::*;
pub use chat_renderer::*;
pub use commands::*;
pub use event_renderer::*;
pub use input_handler::*;
pub use layout_manager::*;

use std::collections::BTreeSet;

use ndc_core::redaction::RedactionMode;

// ── ReplVisualizationState ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReplVisualizationState {
    pub show_thinking: bool,
    pub show_tool_details: bool,
    pub expand_tool_cards: bool,
    pub live_events_enabled: bool,
    pub show_usage_metrics: bool,
    pub verbosity: DisplayVerbosity,
    pub last_emitted_round: usize,
    pub timeline_limit: usize,
    pub timeline_cache: Vec<ndc_core::AgentExecutionEvent>,
    pub redaction_mode: RedactionMode,
    pub hidden_thinking_round_hints: BTreeSet<usize>,
    pub current_workflow_stage: Option<String>,
    pub current_workflow_stage_index: Option<u32>,
    pub current_workflow_stage_total: Option<u32>,
    pub current_workflow_stage_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub session_token_total: u64,
    pub latest_round_token_total: u64,
    pub permission_blocked: bool,
    pub permission_pending_message: Option<String>,
    pub show_todo_panel: bool,
    pub todo_items: Vec<TodoItem>,
    pub todo_scroll_offset: usize,
    /// Set to true when a TodoStateChange event is received, signaling sidebar refresh needed.
    pub todo_sidebar_dirty: bool,
}

impl ReplVisualizationState {
    pub fn new(show_thinking: bool) -> Self {
        let show_thinking = env_bool("NDC_DISPLAY_THINKING").unwrap_or(show_thinking);
        let show_tool_details = env_bool("NDC_TOOL_DETAILS").unwrap_or(false);
        let expand_tool_cards = env_bool("NDC_TOOL_CARDS_EXPANDED").unwrap_or(false);
        let live_events_enabled = env_bool("NDC_REPL_LIVE_EVENTS").unwrap_or(true);
        let show_usage_metrics = env_bool("NDC_REPL_SHOW_USAGE").unwrap_or(true);
        let timeline_limit = env_usize("NDC_TIMELINE_LIMIT").unwrap_or(40).max(1);
        let verbosity = std::env::var("NDC_DISPLAY_VERBOSITY")
            .ok()
            .and_then(|v| DisplayVerbosity::parse(&v))
            .unwrap_or(DisplayVerbosity::Compact);
        Self {
            show_thinking,
            show_tool_details,
            expand_tool_cards,
            live_events_enabled,
            show_usage_metrics,
            verbosity,
            last_emitted_round: 0,
            timeline_limit,
            timeline_cache: Vec::new(),
            redaction_mode: RedactionMode::from_env(),
            hidden_thinking_round_hints: BTreeSet::new(),
            current_workflow_stage: None,
            current_workflow_stage_index: None,
            current_workflow_stage_total: None,
            current_workflow_stage_started_at: None,
            session_token_total: 0,
            latest_round_token_total: 0,
            permission_blocked: false,
            permission_pending_message: None,
            show_todo_panel: true,
            todo_items: Vec::new(),
            todo_scroll_offset: 0,
            todo_sidebar_dirty: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[test]
    fn test_visualization_state_default() {
        with_env_overrides(
            &[
                ("NDC_DISPLAY_THINKING", None),
                ("NDC_TOOL_DETAILS", None),
                ("NDC_TOOL_CARDS_EXPANDED", None),
                ("NDC_REPL_LIVE_EVENTS", None),
                ("NDC_TIMELINE_LIMIT", None),
                ("NDC_DISPLAY_VERBOSITY", None),
            ],
            || {
                let state = ReplVisualizationState::new(false);
                assert!(!state.show_thinking);
                assert!(!state.show_tool_details);
                assert!(!state.expand_tool_cards);
                assert!(state.live_events_enabled);
                assert_eq!(state.timeline_limit, 40);
                assert!(state.timeline_cache.is_empty());
                assert!(state.hidden_thinking_round_hints.is_empty());
                assert!(state.current_workflow_stage_index.is_none());
                assert!(state.current_workflow_stage_total.is_none());
                assert!(state.current_workflow_stage_started_at.is_none());
                assert!(!state.permission_blocked);
                assert!(matches!(state.verbosity, DisplayVerbosity::Compact));
                assert_eq!(state.last_emitted_round, 0);
            },
        );
    }

    #[test]
    fn test_visualization_state_from_env() {
        with_env_overrides(
            &[
                ("NDC_DISPLAY_THINKING", Some("true")),
                ("NDC_TOOL_DETAILS", Some("1")),
                ("NDC_TOOL_CARDS_EXPANDED", Some("true")),
                ("NDC_REPL_LIVE_EVENTS", Some("false")),
                ("NDC_TIMELINE_LIMIT", Some("88")),
                ("NDC_DISPLAY_VERBOSITY", Some("verbose")),
            ],
            || {
                let state = ReplVisualizationState::new(false);
                assert!(state.show_thinking);
                assert!(state.show_tool_details);
                assert!(state.expand_tool_cards);
                assert!(!state.live_events_enabled);
                assert_eq!(state.timeline_limit, 88);
                assert!(matches!(state.verbosity, DisplayVerbosity::Verbose));
            },
        );
    }
}
