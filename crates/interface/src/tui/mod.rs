//! TUI module — chat rendering, input handling, layout management.
//!
//! This module owns the visual and input sub-systems of the TUI REPL.
//! `repl.rs` uses `crate::tui::*` to access all exported items.

use std::collections::BTreeSet;

use crate::redaction::RedactionMode;

mod app;
mod chat_renderer;
mod commands;
mod event_renderer;
mod input_handler;
mod layout_manager;
pub(crate) mod scene;
#[cfg(test)]
pub(crate) mod test_helpers;

// Re-export all pub(crate) items for consumers (repl.rs, tests, etc.)
// Also makes items available via `super::` for sub-module cross-references.
pub(crate) use app::*;
pub(crate) use chat_renderer::*;
pub(crate) use commands::*;
pub(crate) use event_renderer::*;
pub(crate) use input_handler::*;
pub(crate) use layout_manager::*;

// ── ReplVisualizationState (moved from repl.rs) ──────────────────────

#[derive(Debug, Clone)]
pub(crate) struct ReplVisualizationState {
    pub(crate) show_thinking: bool,
    pub(crate) show_tool_details: bool,
    pub(crate) expand_tool_cards: bool,
    pub(crate) live_events_enabled: bool,
    pub(crate) show_usage_metrics: bool,
    pub(crate) verbosity: DisplayVerbosity,
    pub(crate) last_emitted_round: usize,
    pub(crate) timeline_limit: usize,
    pub(crate) timeline_cache: Vec<ndc_core::AgentExecutionEvent>,
    pub(crate) redaction_mode: RedactionMode,
    pub(crate) hidden_thinking_round_hints: BTreeSet<usize>,
    pub(crate) current_workflow_stage: Option<String>,
    pub(crate) current_workflow_stage_index: Option<u32>,
    pub(crate) current_workflow_stage_total: Option<u32>,
    pub(crate) current_workflow_stage_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) session_token_total: u64,
    pub(crate) latest_round_token_total: u64,
    pub(crate) permission_blocked: bool,
    pub(crate) permission_pending_message: Option<String>,
}

impl ReplVisualizationState {
    pub(crate) fn new(show_thinking: bool) -> Self {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::test_helpers::*;

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
