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

pub(crate) fn sync_todo_sidebar_from_event(
    viz_state: &mut ReplVisualizationState,
    event: &ndc_core::AgentExecutionEvent,
) {
    match event.kind {
        ndc_core::AgentExecutionEventKind::PlanningComplete => {
            if let Some(titles) = parse_planning_todos(&event.message)
                && !titles.is_empty()
            {
                viz_state.todo_items = titles
                    .into_iter()
                    .enumerate()
                    .map(|(i, title)| TodoItem {
                        id: format!("event-planned-{}", i + 1),
                        index: i + 1,
                        title,
                        state: TodoState::Pending,
                    })
                    .collect();
                viz_state.todo_scroll_offset = 0;
            }
        }
        ndc_core::AgentExecutionEventKind::TodoStateChange => {
            if let Some((index, title, state)) = parse_todo_state_change(&event.message) {
                upsert_todo_item(viz_state, index, title, state);
            }
            viz_state.todo_sidebar_dirty = true;
        }
        _ => {}
    }
}

fn parse_planning_todos(message: &str) -> Option<Vec<String>> {
    let payload = message.strip_prefix("planning_complete:")?.trim();
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let todos = value.get("todos")?;
    serde_json::from_value::<Vec<String>>(todos.clone()).ok()
}

fn parse_todo_state_change(message: &str) -> Option<(usize, String, TodoState)> {
    let payload = message.strip_prefix("todo_state:")?.trim();
    let (lhs, rhs) = payload.rsplit_once("->")?;
    let state = match rhs.trim().to_ascii_lowercase().as_str() {
        "pending" => TodoState::Pending,
        "in_progress" | "inprogress" => TodoState::InProgress,
        "completed" => TodoState::Completed,
        "failed" => TodoState::Failed,
        "cancelled" | "canceled" => TodoState::Cancelled,
        _ => return None,
    };

    let lhs = lhs.trim();
    let (index, title) = if let Some(rest) = lhs.strip_prefix('#') {
        let (num, title) = rest.split_once(' ')?;
        (num.parse::<usize>().ok()?, title.trim().to_string())
    } else if let Some(rest) = lhs.strip_prefix('[') {
        let (num, rest) = rest.split_once(']')?;
        (num.parse::<usize>().ok()?, rest.trim().to_string())
    } else {
        return None;
    };

    Some((index, title, state))
}

fn upsert_todo_item(
    viz_state: &mut ReplVisualizationState,
    index: usize,
    title: String,
    state: TodoState,
) {
    if let Some(existing) = viz_state.todo_items.iter_mut().find(|t| t.index == index) {
        existing.state = state;
        if !title.is_empty() {
            existing.title = title;
        }
        return;
    }

    viz_state.todo_items.push(TodoItem {
        id: format!("event-todo-{}", index),
        index,
        title: if title.is_empty() {
            format!("TODO #{}", index)
        } else {
            title
        },
        state,
    });
    viz_state.todo_items.sort_by_key(|t| t.index);
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

    #[test]
    fn test_sync_todo_sidebar_from_planning_event() {
        let mut state = ReplVisualizationState::new(false);
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::PlanningComplete,
            timestamp: chrono::Utc::now(),
            message: "planning_complete: {\"todos\":[\"Task A\",\"Task B\"]}".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };

        sync_todo_sidebar_from_event(&mut state, &event);

        assert_eq!(state.todo_items.len(), 2);
        assert_eq!(state.todo_items[0].title, "Task A");
        assert_eq!(state.todo_items[1].state, TodoState::Pending);
    }

    #[test]
    fn test_sync_todo_sidebar_from_state_change_event() {
        let mut state = ReplVisualizationState::new(false);
        state.todo_items = vec![TodoItem {
            id: "x".to_string(),
            index: 1,
            title: "Task A".to_string(),
            state: TodoState::Pending,
        }];
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::TodoStateChange,
            timestamp: chrono::Utc::now(),
            message: "todo_state: #1 Task A -> completed".to_string(),
            round: 2,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };

        sync_todo_sidebar_from_event(&mut state, &event);

        assert_eq!(state.todo_items[0].state, TodoState::Completed);
        assert!(state.todo_sidebar_dirty);
    }
}
