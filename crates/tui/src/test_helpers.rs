//! Shared test helpers for TUI sub-module tests.

use ratatui::text::Line;
use std::sync::{Mutex, OnceLock};

use super::*;

pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock poisoned")
}

pub fn with_env_overrides<T>(updates: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
    let _guard = env_lock();
    let previous = updates
        .iter()
        .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
        .collect::<Vec<_>>();
    for (key, value) in updates {
        match value {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
    let result = f();
    for (key, old) in previous {
        match old {
            Some(v) => unsafe { std::env::set_var(&key, v) },
            None => unsafe { std::env::remove_var(&key) },
        }
    }
    result
}

pub fn mk_event(
    kind: ndc_core::AgentExecutionEventKind,
    message: &str,
    round: usize,
    tool_name: Option<&str>,
    tool_call_id: Option<&str>,
    duration_ms: Option<u64>,
    is_error: bool,
) -> ndc_core::AgentExecutionEvent {
    ndc_core::AgentExecutionEvent {
        kind,
        timestamp: chrono::Utc::now(),
        message: message.to_string(),
        round,
        tool_name: tool_name.map(|s| s.to_string()),
        tool_call_id: tool_call_id.map(|s| s.to_string()),
        duration_ms,
        is_error,
        workflow_stage: None,
        workflow_detail: None,
        workflow_stage_index: None,
        workflow_stage_total: None,
    }
}

pub fn mk_event_at(
    kind: ndc_core::AgentExecutionEventKind,
    message: &str,
    round: usize,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> ndc_core::AgentExecutionEvent {
    ndc_core::AgentExecutionEvent {
        kind,
        timestamp,
        message: message.to_string(),
        round,
        tool_name: None,
        tool_call_id: None,
        duration_ms: None,
        is_error: false,
        workflow_stage: None,
        workflow_detail: None,
        workflow_stage_index: None,
        workflow_stage_total: None,
    }
}

pub fn render_event_snapshot(
    events: &[ndc_core::AgentExecutionEvent],
    viz: &mut ReplVisualizationState,
) -> Vec<String> {
    let mut out = Vec::new();
    for event in events {
        out.extend(event_to_lines(event, viz));
    }
    out
}

pub fn line_plain(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>()
}

pub fn render_entries_snapshot(
    events: &[ndc_core::AgentExecutionEvent],
    viz: &mut ReplVisualizationState,
) -> Vec<ChatEntry> {
    let mut out = Vec::new();
    for event in events {
        out.extend(event_to_entries(event, viz));
    }
    out
}

pub fn entry_lines_plain(entry: &ChatEntry) -> Vec<String> {
    let theme = TuiTheme::default_dark();
    let mut lines = Vec::new();
    style_chat_entry(entry, &theme, &mut lines);
    lines.iter().map(line_plain).collect()
}
