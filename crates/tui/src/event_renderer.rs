//! Event-to-lines rendering and TUI panel content appenders.
//!
//! Converts `AgentExecutionEvent`s to display strings (legacy mode)
//! and provides helpers to append content to the TUI chat panel.

use ndc_core::redaction::sanitize_text;

use super::*;

/// Convert a single execution event into display lines (legacy/CLI mode).
pub fn event_to_lines(
    event: &ndc_core::AgentExecutionEvent,
    viz_state: &mut ReplVisualizationState,
) -> Vec<String> {
    if !matches!(
        event.kind,
        ndc_core::AgentExecutionEventKind::PermissionAsked
            | ndc_core::AgentExecutionEventKind::Reasoning
    ) {
        viz_state.permission_blocked = false;
        viz_state.permission_pending_message = None;
    }
    let v = viz_state.verbosity;

    // Round separator (Normal/Verbose only)
    let mut lines = Vec::new();
    if matches!(v, DisplayVerbosity::Normal | DisplayVerbosity::Verbose)
        && event.round > viz_state.last_emitted_round
        && event.round > 0
    {
        lines.push(format!("[RoundSep] ── Round {} ──", event.round));
    }
    if event.round > 0 {
        viz_state.last_emitted_round = event.round;
    }

    match event.kind {
        ndc_core::AgentExecutionEventKind::WorkflowStage => {
            if let Some(stage_info) = event.workflow_stage_info() {
                let stage = stage_info.stage;
                viz_state.current_workflow_stage = Some(stage.as_str().to_string());
                viz_state.current_workflow_stage_index = Some(stage_info.index);
                viz_state.current_workflow_stage_total = Some(stage_info.total);
                viz_state.current_workflow_stage_started_at = Some(event.timestamp);
                match v {
                    DisplayVerbosity::Compact => {
                        lines.push(format!("[Stage] {}...", capitalize_stage(stage.as_str())));
                    }
                    DisplayVerbosity::Normal => {
                        let detail = if stage_info.detail.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", stage_info.detail)
                        };
                        lines.push(format!(
                            "[Stage] {}{}",
                            capitalize_stage(stage.as_str()),
                            detail
                        ));
                    }
                    DisplayVerbosity::Verbose => {
                        lines.push(format!("[stage:{}]", stage));
                        lines.push(format!(
                            "[Workflow][r{}] {}",
                            event.round,
                            sanitize_text(&event.message, viz_state.redaction_mode)
                        ));
                    }
                }
            } else {
                lines.push(format!(
                    "[Workflow][r{}] {}",
                    event.round,
                    sanitize_text(&event.message, viz_state.redaction_mode)
                ));
            }
        }
        ndc_core::AgentExecutionEventKind::Reasoning => {
            if viz_state.show_thinking {
                lines.push(format!("[Thinking][r{}]", event.round));
                lines.push(format!(
                    "  └─ {}",
                    sanitize_text(&event.message, viz_state.redaction_mode)
                ));
            } else if !viz_state.hidden_thinking_round_hints.contains(&event.round) {
                viz_state.hidden_thinking_round_hints.insert(event.round);
                lines.push(format!(
                    "[Thinking][r{}] (collapsed, use /t or /thinking show)",
                    event.round
                ));
            }
        }
        ndc_core::AgentExecutionEventKind::ToolCallStart => {
            let tool = event.tool_name.as_deref().unwrap_or("unknown");
            match v {
                DisplayVerbosity::Compact => {
                    if let Some(args) = extract_tool_args_preview(&event.message) {
                        let summary = extract_tool_summary(tool, args);
                        if summary.is_empty() {
                            lines.push(format!("[ToolRun] {}", tool));
                        } else {
                            let (s, _) = truncate_output(&summary, 80);
                            lines.push(format!("[ToolRun] {} {}", tool, s));
                        }
                    } else {
                        lines.push(format!("[ToolRun] {}", tool));
                    }
                }
                DisplayVerbosity::Normal => {
                    lines.push(format!("[ToolRun] {}", tool));
                    if let Some(args) = extract_tool_args_preview(&event.message) {
                        let summary = extract_tool_summary(tool, args);
                        if !summary.is_empty() {
                            lines.push(format!(
                                "  └─ {}",
                                sanitize_text(&summary, viz_state.redaction_mode)
                            ));
                        }
                    }
                }
                DisplayVerbosity::Verbose => {
                    lines.push(format!("[Tool][r{}] start {}", event.round, tool));
                    if let Some(args) = extract_tool_args_preview(&event.message) {
                        lines.push(format!(
                            "  └─ input : {}",
                            sanitize_text(args, viz_state.redaction_mode)
                        ));
                    }
                }
            }
        }
        ndc_core::AgentExecutionEventKind::ToolCallEnd => {
            let tool = event.tool_name.as_deref().unwrap_or("unknown");
            let duration = event.duration_ms.map(format_duration_ms);
            let status_icon = if event.is_error { "✗" } else { "✓" };

            match v {
                DisplayVerbosity::Compact => {
                    let dur = duration.map(|d| format!(" ({})", d)).unwrap_or_default();
                    if event.is_error {
                        if let Some(preview) = extract_tool_result_preview(&event.message) {
                            let (msg, _) = truncate_output(
                                &sanitize_text(preview, viz_state.redaction_mode),
                                100,
                            );
                            lines.push(format!(
                                "[ToolEnd] {} {}{} — {}",
                                status_icon, tool, dur, msg
                            ));
                        } else {
                            lines.push(format!("[ToolEnd] {} {}{}", status_icon, tool, dur));
                        }
                    } else if let Some(preview) = extract_tool_result_preview(&event.message) {
                        let (msg, truncated) =
                            truncate_output(&sanitize_text(preview, viz_state.redaction_mode), 100);
                        lines.push(format!("[ToolEnd] {} {}{}", status_icon, tool, dur));
                        let suffix = if truncated { " …" } else { "" };
                        lines.push(format!("  └─ {}{}", msg, suffix));
                    } else {
                        lines.push(format!("[ToolEnd] {} {}{}", status_icon, tool, dur));
                    }
                }
                DisplayVerbosity::Normal => {
                    let dur = duration.map(|d| format!(" ({})", d)).unwrap_or_default();
                    lines.push(format!("[ToolEnd] {} {}{}", status_icon, tool, dur));
                    if let Some(preview) = extract_tool_result_preview(&event.message) {
                        lines.push(format!(
                            "  ├─ {}: {}",
                            if event.is_error { "error " } else { "output" },
                            sanitize_text(preview, viz_state.redaction_mode)
                        ));
                    }
                    if viz_state.expand_tool_cards
                        && let Some(args) = extract_tool_args_preview(&event.message)
                    {
                        lines.push(format!(
                            "  └─ input : {}",
                            sanitize_text(args, viz_state.redaction_mode)
                        ));
                    }
                }
                DisplayVerbosity::Verbose => {
                    lines.push(format!(
                        "[Tool][r{}] {} {}{}",
                        event.round,
                        if event.is_error { "failed" } else { "done" },
                        tool,
                        event
                            .duration_ms
                            .map(|d| format!(" ({}ms)", d))
                            .unwrap_or_default()
                    ));
                    if let Some(preview) = extract_tool_result_preview(&event.message) {
                        lines.push(format!(
                            "  ├─ {}: {}",
                            if event.is_error { "error " } else { "output" },
                            sanitize_text(preview, viz_state.redaction_mode)
                        ));
                    }
                    if viz_state.expand_tool_cards {
                        if let Some(args) = extract_tool_args_preview(&event.message) {
                            lines.push(format!(
                                "  ├─ input : {}",
                                sanitize_text(args, viz_state.redaction_mode)
                            ));
                        }
                        lines.push(format!(
                            "  └─ meta  : call_id={} status={}",
                            event.tool_call_id.as_deref().unwrap_or("-"),
                            if event.is_error { "error" } else { "ok" }
                        ));
                    } else if viz_state.show_tool_details {
                        lines.push("  └─ (collapsed card, use /cards or Ctrl+E)".to_string());
                    }
                }
            }
        }
        ndc_core::AgentExecutionEventKind::TokenUsage => {
            if let Some(usage) = event.token_usage_info() {
                viz_state.latest_round_token_total = usage.total_tokens;
                viz_state.session_token_total = usage.session_total;
            }
            match v {
                DisplayVerbosity::Compact => {}
                DisplayVerbosity::Normal => {
                    if let Some(usage) = event.token_usage_info() {
                        lines.push(format!(
                            "[Usage] tok +{} ({} total)",
                            format_token_count(usage.total_tokens),
                            format_token_count(usage.session_total),
                        ));
                    }
                }
                DisplayVerbosity::Verbose => {
                    lines.push(format!(
                        "[Usage][r{}] {}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode)
                    ));
                }
            }
        }
        ndc_core::AgentExecutionEventKind::PermissionAsked => {
            viz_state.permission_blocked = true;
            viz_state.permission_pending_message =
                Some(sanitize_text(&event.message, viz_state.redaction_mode));
            match v {
                DisplayVerbosity::Compact | DisplayVerbosity::Normal => {
                    let msg = sanitize_text(&event.message, viz_state.redaction_mode);
                    lines.push(format!("[PermBlock] {}", msg));
                    lines.push(
                        "[PermHint] ⓘ Reply in terminal to approve, or set /allow".to_string(),
                    );
                }
                DisplayVerbosity::Verbose => {
                    lines.push(format!(
                        "[Permission][r{}] {}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode)
                    ));
                }
            }
        }
        ndc_core::AgentExecutionEventKind::StepStart
        | ndc_core::AgentExecutionEventKind::StepFinish
        | ndc_core::AgentExecutionEventKind::Verification => match v {
            DisplayVerbosity::Compact => {}
            DisplayVerbosity::Normal => {
                if matches!(event.kind, ndc_core::AgentExecutionEventKind::StepFinish)
                    && event.duration_ms.is_some()
                {
                    lines.push(format!(
                        "[Step][r{}] {}{}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode),
                        event
                            .duration_ms
                            .map(|d| format!(" ({})", format_duration_ms(d)))
                            .unwrap_or_default()
                    ));
                }
            }
            DisplayVerbosity::Verbose => {
                if !viz_state.show_tool_details
                    && matches!(event.kind, ndc_core::AgentExecutionEventKind::StepStart)
                {
                    lines.push(format!("[Agent][r{}] thinking...", event.round));
                } else if viz_state.show_tool_details {
                    lines.push(format!(
                        "[Step][r{}] {}{}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode),
                        event
                            .duration_ms
                            .map(|d| format!(" ({}ms)", d))
                            .unwrap_or_default()
                    ));
                }
            }
        },
        ndc_core::AgentExecutionEventKind::Error => {
            lines.push(format!(
                "[Error][r{}] {}",
                event.round,
                sanitize_text(&event.message, viz_state.redaction_mode)
            ));
        }
        ndc_core::AgentExecutionEventKind::SessionStatus
        | ndc_core::AgentExecutionEventKind::Text
        | ndc_core::AgentExecutionEventKind::TodoStateChange
        | ndc_core::AgentExecutionEventKind::AnalysisComplete
        | ndc_core::AgentExecutionEventKind::PlanningComplete
        | ndc_core::AgentExecutionEventKind::TodoExecutionStart
        | ndc_core::AgentExecutionEventKind::TodoExecutionEnd
        | ndc_core::AgentExecutionEventKind::Report => {}
    }
    lines
}

pub fn append_recent_thinking(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
    let total = viz_state.timeline_cache.len();
    let start = total.saturating_sub(viz_state.timeline_limit);
    push_text_entry(entries, "");
    push_text_entry(
        entries,
        &format!(
            "Recent Thinking (last {} events):",
            viz_state.timeline_limit
        ),
    );
    let mut count = 0usize;
    for event in viz_state.timeline_cache.iter().skip(start) {
        if !matches!(event.kind, ndc_core::AgentExecutionEventKind::Reasoning) {
            continue;
        }
        push_text_entry(
            entries,
            &format!(
                "  - r{} {} | {}",
                event.round,
                event.timestamp.format("%H:%M:%S"),
                sanitize_text(&event.message, viz_state.redaction_mode),
            ),
        );
        count += 1;
    }
    if count == 0 {
        push_text_entry(entries, "  (no thinking events yet)");
    }
}

pub fn append_recent_timeline(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
    push_text_entry(entries, "");
    push_text_entry(
        entries,
        &format!(
            "Recent Execution Timeline (last {}):",
            viz_state.timeline_limit
        ),
    );
    let total = viz_state.timeline_cache.len();
    let start = total.saturating_sub(viz_state.timeline_limit);
    if start == total {
        push_text_entry(entries, "  (empty)");
        return;
    }
    let grouped = group_timeline_by_stage(&viz_state.timeline_cache[start..]);
    for (stage, events) in grouped {
        push_text_entry(entries, &format!("  [stage:{}]", stage));
        for event in events {
            push_text_entry(
                entries,
                &format!(
                    "    - r{} {} | {}{}",
                    event.round,
                    event.timestamp.format("%H:%M:%S"),
                    sanitize_text(&event.message, viz_state.redaction_mode),
                    event
                        .duration_ms
                        .map(|d| format!(" ({}ms)", d))
                        .unwrap_or_default()
                ),
            );
        }
    }
}

pub fn append_workflow_overview(
    entries: &mut Vec<ChatEntry>,
    viz_state: &ReplVisualizationState,
    mode: WorkflowOverviewMode,
) {
    push_text_entry(entries, "");
    push_text_entry(
        entries,
        &format!(
            "Workflow Overview ({}) current={} progress={}",
            mode.as_str(),
            viz_state.current_workflow_stage.as_deref().unwrap_or("-"),
            workflow_progress_descriptor(
                viz_state.current_workflow_stage.as_deref(),
                viz_state.current_workflow_stage_index,
                viz_state.current_workflow_stage_total,
            )
        ),
    );
    let summary =
        compute_workflow_progress_summary(viz_state.timeline_cache.as_slice(), chrono::Utc::now());
    if summary.history_may_be_partial {
        push_text_entry(
            entries,
            &format!(
                "[Warning] workflow history may be partial (cache cap={} events)",
                TIMELINE_CACHE_MAX_EVENTS
            ),
        );
    }
    push_text_entry(entries, "Workflow Progress:");
    for stage in WORKFLOW_STAGE_ORDER {
        let metrics = summary.stages.get(*stage).copied().unwrap_or_default();
        let active_ms = if summary.current_stage.as_deref() == Some(*stage) {
            summary.current_stage_active_ms
        } else {
            0
        };
        push_text_entry(
            entries,
            &format!(
                "  - {} count={} total_ms={} active_ms={}",
                stage, metrics.count, metrics.total_ms, active_ms
            ),
        );
    }
    if mode == WorkflowOverviewMode::Verbose {
        let total = viz_state.timeline_cache.len();
        let start = total.saturating_sub(viz_state.timeline_limit);
        let mut count = 0usize;
        for event in viz_state.timeline_cache.iter().skip(start) {
            if !matches!(event.kind, ndc_core::AgentExecutionEventKind::WorkflowStage) {
                continue;
            }
            push_text_entry(
                entries,
                &format!(
                    "  - r{} {} | {}",
                    event.round,
                    event.timestamp.format("%H:%M:%S"),
                    sanitize_text(&event.message, viz_state.redaction_mode),
                ),
            );
            count += 1;
        }
        if count == 0 {
            push_text_entry(entries, "  (no workflow stage events yet)");
        }
    } else {
        push_text_entry(
            entries,
            "  (use /workflow verbose to inspect stage event timeline)",
        );
    }
}

pub fn append_token_usage(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
    push_text_entry(entries, "");
    push_text_entry(
        entries,
        &format!(
            "Token Usage: round_total={} session_total={} display={}",
            viz_state.latest_round_token_total,
            viz_state.session_token_total,
            if viz_state.show_usage_metrics {
                "ON"
            } else {
                "OFF"
            }
        ),
    );
}

pub fn append_runtime_metrics(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
    let metrics = compute_runtime_metrics(viz_state.timeline_cache.as_slice());
    push_text_entry(entries, "");
    push_text_entry(entries, "Runtime Metrics:");
    push_text_entry(
        entries,
        &format!(
            "  - workflow_current={}",
            viz_state.current_workflow_stage.as_deref().unwrap_or("-")
        ),
    );
    push_text_entry(
        entries,
        &format!(
            "  - blocked_on_permission={}",
            if viz_state.permission_blocked {
                "yes"
            } else {
                "no"
            }
        ),
    );
    push_text_entry(
        entries,
        &format!(
            "  - token_round_total={} token_session_total={} display={}",
            viz_state.latest_round_token_total,
            viz_state.session_token_total,
            if viz_state.show_usage_metrics {
                "ON"
            } else {
                "OFF"
            }
        ),
    );
    push_text_entry(
        entries,
        &format!(
            "  - tools_total={} tools_failed={} tool_error_rate={}%",
            metrics.tool_calls_total,
            metrics.tool_calls_failed,
            metrics.tool_error_rate_percent()
        ),
    );
    push_text_entry(
        entries,
        &format!(
            "  - tool_avg_duration_ms={}",
            metrics
                .avg_tool_duration_ms()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
    );
    push_text_entry(
        entries,
        &format!(
            "  - permission_waits={} error_events={}",
            metrics.permission_waits, metrics.error_events
        ),
    );
}

pub fn apply_tui_shortcut_action(
    action: TuiShortcutAction,
    viz_state: &mut ReplVisualizationState,
    entries: &mut Vec<ChatEntry>,
) {
    match action {
        TuiShortcutAction::ToggleThinking => {
            viz_state.show_thinking = !viz_state.show_thinking;
            if viz_state.show_thinking {
                viz_state.hidden_thinking_round_hints.clear();
            }
            toggle_all_reasoning_blocks(entries);
            push_text_entry(
                entries,
                &format!(
                    "[OK] Thinking: {}",
                    if viz_state.show_thinking {
                        "ON"
                    } else {
                        "OFF (collapsed)"
                    }
                ),
            );
        }
        TuiShortcutAction::ToggleDetails => {
            viz_state.verbosity = viz_state.verbosity.next();
            push_text_entry(
                entries,
                &format!("[OK] Verbosity: {}", viz_state.verbosity.label()),
            );
        }
        TuiShortcutAction::ToggleToolCards => {
            viz_state.expand_tool_cards = !viz_state.expand_tool_cards;
            toggle_all_tool_cards(entries);
            push_text_entry(
                entries,
                &format!(
                    "[OK] Tool cards: {}",
                    if viz_state.expand_tool_cards {
                        "EXPANDED"
                    } else {
                        "COLLAPSED"
                    }
                ),
            );
        }
        TuiShortcutAction::ShowRecentThinking => {
            append_recent_thinking(entries, viz_state);
        }
        TuiShortcutAction::ShowTimeline => {
            append_recent_timeline(entries, viz_state);
        }
        TuiShortcutAction::ClearPanel => {
            entries.clear();
        }
        TuiShortcutAction::ToggleTodoPanel => {
            viz_state.show_todo_panel = !viz_state.show_todo_panel;
            push_text_entry(
                entries,
                &format!(
                    "[OK] TODO panel: {}",
                    if viz_state.show_todo_panel {
                        "VISIBLE"
                    } else {
                        "HIDDEN"
                    }
                ),
            );
        }
    }
}

pub fn render_execution_events(
    events: &[ndc_core::AgentExecutionEvent],
    viz_state: &mut ReplVisualizationState,
) {
    for event in events {
        for line in event_to_lines(event, viz_state) {
            println!("{}", line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn test_show_recent_thinking_empty() {
        show_recent_thinking(&[], 10, RedactionMode::Basic);
    }

    #[test]
    fn test_append_recent_thinking_to_logs() {
        let mut viz = ReplVisualizationState::new(false);
        viz.timeline_limit = 10;
        viz.timeline_cache.push(ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::Reasoning,
            timestamp: chrono::Utc::now(),
            message: "plan: inspect files".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        });
        let mut entries: Vec<ChatEntry> = Vec::new();
        append_recent_thinking(&mut entries, &viz);
        let joined = entries_to_plain_text(&entries);
        assert!(joined.contains("Recent Thinking"));
        assert!(joined.contains("plan: inspect files"));
    }

    #[test]
    fn test_append_recent_timeline_to_logs() {
        let mut viz = ReplVisualizationState::new(false);
        viz.timeline_limit = 2;
        viz.timeline_cache.push(ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::StepStart,
            timestamp: chrono::Utc::now(),
            message: "llm_round_1_start".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        });
        viz.timeline_cache.push(ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::ToolCallEnd,
            timestamp: chrono::Utc::now(),
            message: "tool_call_end: list (ok) | result_preview: file".to_string(),
            round: 1,
            tool_name: Some("list".to_string()),
            tool_call_id: Some("call-x".to_string()),
            duration_ms: Some(3),
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        });
        let mut entries: Vec<ChatEntry> = Vec::new();
        append_recent_timeline(&mut entries, &viz);
        let joined = entries_to_plain_text(&entries);
        assert!(joined.contains("Recent Execution Timeline"));
        assert!(joined.contains("[stage:"));
        assert!(joined.contains("llm_round_1_start"));
        assert!(joined.contains("result_preview"));
    }

    #[test]
    fn test_event_helpers_parse_workflow_and_usage_metrics() {
        let workflow = mk_event(
            ndc_core::AgentExecutionEventKind::WorkflowStage,
            "workflow_stage: executing | llm_round_start",
            1,
            None,
            None,
            None,
            false,
        );
        let workflow_info = workflow.workflow_stage_info().expect("workflow info");
        assert_eq!(workflow_info.stage, ndc_core::AgentWorkflowStage::Executing);
        assert_eq!(workflow_info.detail, "llm_round_start");

        let usage_event = mk_event(
            ndc_core::AgentExecutionEventKind::TokenUsage,
            "token_usage: source=provider prompt=12 completion=34 total=46 | session_prompt_total=12 session_completion_total=34 session_total=46",
            1,
            None,
            None,
            None,
            false,
        );
        let usage = usage_event.token_usage_info().expect("usage info");
        assert_eq!(usage.source, "provider");
        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.total_tokens, 46);
        assert_eq!(usage.session_total, 46);
    }

    #[test]
    fn test_event_to_lines_workflow_stage_updates_state() {
        let mut viz = ReplVisualizationState::new(false);
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::WorkflowStage,
            "workflow_stage: executing | tool_calls_planned",
            2,
            None,
            None,
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert_eq!(viz.current_workflow_stage.as_deref(), Some("executing"));
        assert_eq!(viz.current_workflow_stage_index, Some(5));
        assert_eq!(
            viz.current_workflow_stage_total,
            Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES)
        );
        assert!(viz.current_workflow_stage_started_at.is_some());
        assert!(!viz.permission_blocked);
        // Compact mode: single [Stage] line
        assert!(lines.iter().any(|line| line.contains("[Stage]")));
        assert!(lines.iter().any(|line| line.contains("Executing")));
    }

    #[test]
    fn test_event_to_lines_workflow_stage_prefers_structured_payload() {
        let mut viz = ReplVisualizationState::new(false);
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "stage changed".to_string(),
            round: 3,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Verifying),
            workflow_detail: Some("quality_gate".to_string()),
            workflow_stage_index: Some(4),
            workflow_stage_total: Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES),
        };
        let lines = event_to_lines(&event, &mut viz);
        assert_eq!(viz.current_workflow_stage.as_deref(), Some("verifying"));
        assert_eq!(viz.current_workflow_stage_index, Some(4));
        assert_eq!(
            viz.current_workflow_stage_total,
            Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES)
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("[Stage]") && line.contains("Verifying"))
        );
    }

    #[test]
    fn test_event_to_lines_token_usage_updates_state() {
        let mut viz = ReplVisualizationState::new(false);
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::TokenUsage,
            "token_usage: source=provider prompt=10 completion=5 total=15 | session_prompt_total=22 session_completion_total=11 session_total=33",
            3,
            None,
            None,
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert_eq!(viz.latest_round_token_total, 15);
        assert_eq!(viz.session_token_total, 33);
        // Compact mode: tokens hidden (shown in status bar)
        assert!(lines.is_empty());
    }

    #[test]
    fn test_event_to_lines_permission_sets_and_clears_blocked_state() {
        let mut viz = ReplVisualizationState::new(false);
        let permission = mk_event(
            ndc_core::AgentExecutionEventKind::PermissionAsked,
            "permission_asked: write requires approval",
            2,
            Some("write"),
            Some("call-1"),
            None,
            true,
        );
        let _ = event_to_lines(&permission, &mut viz);
        assert!(viz.permission_blocked);

        let tool_end = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallEnd,
            "tool_call_end: write (error) | result_preview: denied",
            2,
            Some("write"),
            Some("call-1"),
            Some(5),
            true,
        );
        let _ = event_to_lines(&tool_end, &mut viz);
        assert!(!viz.permission_blocked);
    }

    #[test]
    fn test_append_runtime_metrics_to_logs() {
        let mut viz = ReplVisualizationState::new(false);
        viz.current_workflow_stage = Some("executing".to_string());
        viz.permission_blocked = true;
        viz.latest_round_token_total = 15;
        viz.session_token_total = 45;
        viz.timeline_cache = vec![mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallEnd,
            "tool_call_end: list (ok)",
            1,
            Some("list"),
            Some("call-1"),
            Some(3),
            false,
        )];
        let mut entries: Vec<ChatEntry> = Vec::new();
        append_runtime_metrics(&mut entries, &viz);
        let joined = entries_to_plain_text(&entries);
        assert!(joined.contains("Runtime Metrics"));
        assert!(joined.contains("workflow_current=executing"));
        assert!(joined.contains("blocked_on_permission=yes"));
        assert!(joined.contains("token_round_total=15"));
        assert!(joined.contains("tools_total=1"));
    }

    #[test]
    fn test_append_workflow_overview_to_logs_includes_progress() {
        let mut viz = ReplVisualizationState::new(false);
        viz.current_workflow_stage = Some("executing".to_string());
        viz.timeline_cache = vec![
            mk_event(
                ndc_core::AgentExecutionEventKind::WorkflowStage,
                "workflow_stage: planning | build_prompt_and_context",
                0,
                None,
                None,
                None,
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::WorkflowStage,
                "workflow_stage: executing | llm_round_start",
                1,
                None,
                None,
                None,
                false,
            ),
        ];
        let mut entries: Vec<ChatEntry> = Vec::new();
        append_workflow_overview(&mut entries, &viz, WorkflowOverviewMode::Verbose);
        let joined = entries_to_plain_text(&entries);
        assert!(joined.contains("Workflow Overview (verbose) current=executing progress=62%(5/8)"));
        assert!(joined.contains("Workflow Progress"));
        assert!(joined.contains("planning count="));
        assert!(joined.contains("executing count="));
    }

    #[test]
    fn test_append_workflow_overview_to_logs_compact_hides_stage_events() {
        let mut viz = ReplVisualizationState::new(false);
        viz.current_workflow_stage = Some("executing".to_string());
        viz.timeline_cache = vec![mk_event(
            ndc_core::AgentExecutionEventKind::WorkflowStage,
            "workflow_stage: executing | llm_round_start",
            1,
            None,
            None,
            None,
            false,
        )];
        let mut entries: Vec<ChatEntry> = Vec::new();
        append_workflow_overview(&mut entries, &viz, WorkflowOverviewMode::Compact);
        let joined = entries_to_plain_text(&entries);
        assert!(joined.contains("Workflow Overview (compact)"));
        assert!(joined.contains("use /workflow verbose"));
        assert!(!joined.contains("r1"));
    }

    #[test]
    fn test_apply_tui_shortcut_action_runtime_toggle_and_clear() {
        with_env_overrides(
            &[
                ("NDC_DISPLAY_THINKING", None),
                ("NDC_TOOL_DETAILS", None),
                ("NDC_TOOL_CARDS_EXPANDED", None),
                ("NDC_DISPLAY_VERBOSITY", None),
            ],
            || {
                let mut viz = ReplVisualizationState::new(false);
                let mut entries: Vec<ChatEntry> = vec![ChatEntry::SystemNote("seed".to_string())];

                apply_tui_shortcut_action(
                    TuiShortcutAction::ToggleThinking,
                    &mut viz,
                    &mut entries,
                );
                assert!(viz.show_thinking);
                let joined = entries_to_plain_text(&entries);
                assert!(joined.contains("[OK] Thinking"));

                // ToggleDetails now cycles verbosity: Compact → Normal
                apply_tui_shortcut_action(TuiShortcutAction::ToggleDetails, &mut viz, &mut entries);
                assert!(matches!(viz.verbosity, DisplayVerbosity::Normal));
                let joined = entries_to_plain_text(&entries);
                assert!(joined.contains("[OK] Verbosity"));

                apply_tui_shortcut_action(
                    TuiShortcutAction::ToggleToolCards,
                    &mut viz,
                    &mut entries,
                );
                assert!(viz.expand_tool_cards);
                let joined = entries_to_plain_text(&entries);
                assert!(joined.contains("[OK] Tool cards"));

                apply_tui_shortcut_action(TuiShortcutAction::ClearPanel, &mut viz, &mut entries);
                assert!(entries.is_empty());
            },
        );
    }

    #[test]
    fn test_apply_tui_shortcut_action_show_timeline_from_cache() {
        let mut viz = ReplVisualizationState::new(false);
        viz.timeline_limit = 5;
        viz.timeline_cache.push(mk_event(
            ndc_core::AgentExecutionEventKind::StepFinish,
            "llm_round_1_finish",
            1,
            None,
            None,
            Some(12),
            false,
        ));

        let mut entries: Vec<ChatEntry> = Vec::new();
        apply_tui_shortcut_action(TuiShortcutAction::ShowTimeline, &mut viz, &mut entries);

        let joined = entries_to_plain_text(&entries);
        assert!(joined.contains("Recent Execution Timeline"));
        assert!(joined.contains("llm_round_1_finish"));
    }

    #[test]
    fn test_runtime_shortcut_pipeline_ctrl_t_and_scroll_reset() {
        use crossterm::event::KeyEvent;
        with_env_overrides(
            &[
                ("NDC_DISPLAY_THINKING", None),
                ("NDC_DISPLAY_VERBOSITY", None),
            ],
            || {
                let map = ReplTuiKeymap {
                    toggle_thinking: 't',
                    toggle_details: 'd',
                    toggle_tool_cards: 'e',
                    show_recent_thinking: 'y',
                    show_timeline: 'i',
                    clear_panel: 'l',
                    toggle_todo: 'o',
                };
                let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL);
                let action = detect_tui_shortcut(&key, &map).expect("shortcut action");
                let mut viz = ReplVisualizationState::new(false);
                let mut entries: Vec<ChatEntry> = (0..30)
                    .map(|i| ChatEntry::SystemNote(format!("line-{}", i)))
                    .collect();
                let total_lines = total_display_lines(&entries);
                let before_scroll = calc_log_scroll(total_lines, 10);
                assert!(before_scroll > 0);

                apply_tui_shortcut_action(action, &mut viz, &mut entries);
                assert!(viz.show_thinking);

                apply_tui_shortcut_action(TuiShortcutAction::ClearPanel, &mut viz, &mut entries);
                let after_scroll = calc_log_scroll(total_display_lines(&entries), 10);
                assert_eq!(after_scroll, 0);
            },
        );
    }

    #[test]
    fn test_event_to_lines_reasoning_block_expanded() {
        let mut viz = ReplVisualizationState::new(true);
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::Reasoning,
            timestamp: chrono::Utc::now(),
            message: "inspect and plan".to_string(),
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
        let lines = event_to_lines(&event, &mut viz);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("[Thinking][r2]"));
        assert!(lines[1].contains("inspect and plan"));
    }

    #[test]
    fn test_event_to_lines_tool_start_with_input_details() {
        let mut viz = ReplVisualizationState::new(false);
        viz.expand_tool_cards = true;
        // Verbose mode shows full detail like the old format
        viz.verbosity = DisplayVerbosity::Verbose;
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::ToolCallStart,
            timestamp: chrono::Utc::now(),
            message: "tool_call_start: read | args_preview: {\"path\":\"README.md\"}".to_string(),
            round: 3,
            tool_name: Some("read".to_string()),
            tool_call_id: Some("call-1".to_string()),
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let lines = event_to_lines(&event, &mut viz);
        assert!(lines.iter().any(|l| l.contains("start read")));
        assert!(lines.iter().any(|l| l.contains("input")));
        assert!(lines.iter().any(|l| l.contains("README.md")));
    }

    #[test]
    fn test_event_to_lines_tool_end_collapsed_hint() {
        let mut viz = ReplVisualizationState::new(false);
        viz.show_tool_details = true;
        viz.expand_tool_cards = false;
        // Verbose mode shows the collapsed card hint
        viz.verbosity = DisplayVerbosity::Verbose;
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::ToolCallEnd,
            timestamp: chrono::Utc::now(),
            message:
                "tool_call_end: read (ok) | args_preview: {\"path\":\"README.md\"} | result_preview: ok"
                    .to_string(),
            round: 4,
            tool_name: Some("read".to_string()),
            tool_call_id: Some("call-2".to_string()),
            duration_ms: Some(4),
            is_error: false,
        workflow_stage: None,
        workflow_detail: None,
        workflow_stage_index: None,
        workflow_stage_total: None,
        };
        let lines = event_to_lines(&event, &mut viz);
        assert!(lines.iter().any(|l| l.contains("output")));
        assert!(lines.iter().any(|l| l.contains("collapsed card")));
    }

    #[test]
    fn test_event_to_lines_tool_end_error_label() {
        let mut viz = ReplVisualizationState::new(false);
        viz.expand_tool_cards = true;
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::ToolCallEnd,
            timestamp: chrono::Utc::now(),
            message: "tool_call_end: write (error) | args_preview: {\"path\":\"x\"} | result_preview: Error: denied".to_string(),
            round: 5,
            tool_name: Some("write".to_string()),
            tool_call_id: Some("call-3".to_string()),
            duration_ms: Some(5),
            is_error: true,
        workflow_stage: None,
        workflow_detail: None,
        workflow_stage_index: None,
        workflow_stage_total: None,
        };
        let lines = event_to_lines(&event, &mut viz);
        // Compact error: [ToolEnd] ✗ write (5ms) — Error: denied
        assert!(lines.iter().any(|l| l.contains("✗")));
        assert!(lines.iter().any(|l| l.contains("denied")));
    }

    #[test]
    fn test_event_render_snapshot_collapsed_mode() {
        let mut viz = ReplVisualizationState::new(false);
        viz.show_tool_details = false;
        viz.expand_tool_cards = false;
        // Default is Compact
        let events = vec![
            mk_event(
                ndc_core::AgentExecutionEventKind::Reasoning,
                "plan and inspect",
                1,
                None,
                None,
                None,
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallStart,
                "tool_call_start: list | args_preview: {\"path\":\".\"}",
                1,
                Some("list"),
                Some("call-1"),
                None,
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallEnd,
                "tool_call_end: list (ok) | args_preview: {\"path\":\".\"} | result_preview: Cargo.toml",
                1,
                Some("list"),
                Some("call-1"),
                Some(2),
                false,
            ),
        ];
        let actual = render_event_snapshot(&events, &mut viz);
        let expected = vec![
            "[Thinking][r1] (collapsed, use /t or /thinking show)".to_string(),
            "[ToolRun] list .".to_string(),
            "[ToolEnd] ✓ list (2ms)".to_string(),
            "  └─ Cargo.toml".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_event_render_snapshot_expanded_mode() {
        let mut viz = ReplVisualizationState::new(true);
        viz.show_tool_details = true;
        viz.expand_tool_cards = true;
        viz.verbosity = DisplayVerbosity::Verbose;
        let events = vec![
            mk_event(
                ndc_core::AgentExecutionEventKind::Reasoning,
                "read file then summarize",
                2,
                None,
                None,
                None,
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallStart,
                "tool_call_start: read | args_preview: {\"path\":\"README.md\"}",
                2,
                Some("read"),
                Some("call-2"),
                None,
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallEnd,
                "tool_call_end: read (ok) | args_preview: {\"path\":\"README.md\"} | result_preview: # Title",
                2,
                Some("read"),
                Some("call-2"),
                Some(7),
                false,
            ),
        ];
        let actual = render_event_snapshot(&events, &mut viz);
        let expected = vec![
            "[RoundSep] ── Round 2 ──".to_string(),
            "[Thinking][r2]".to_string(),
            "  └─ read file then summarize".to_string(),
            "[Tool][r2] start read".to_string(),
            "  └─ input : {\"path\":\"README.md\"}".to_string(),
            "[Tool][r2] done read (7ms)".to_string(),
            "  ├─ output: # Title".to_string(),
            "  ├─ input : {\"path\":\"README.md\"}".to_string(),
            "  └─ meta  : call_id=call-2 status=ok".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_repl_snapshot_switch_combinations() {
        let reasoning = mk_event(
            ndc_core::AgentExecutionEventKind::Reasoning,
            "analyze project structure",
            1,
            None,
            None,
            None,
            false,
        );
        let step_start = mk_event(
            ndc_core::AgentExecutionEventKind::StepStart,
            "llm_round_1_start",
            1,
            None,
            None,
            None,
            false,
        );
        let tool_end = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallEnd,
            "tool_call_end: list (ok) | args_preview: {\"path\":\".\"} | result_preview: Cargo.toml",
            1,
            Some("list"),
            Some("call-9"),
            Some(4),
            false,
        );
        let events = vec![reasoning, step_start, tool_end];

        // Compact mode (default): steps hidden, tool lines compact
        let mut collapsed = ReplVisualizationState::new(false);
        collapsed.show_tool_details = false;
        collapsed.expand_tool_cards = false;
        let collapsed_lines = render_event_snapshot(&events, &mut collapsed);
        let collapsed_expected = vec![
            "[Thinking][r1] (collapsed, use /t or /thinking show)".to_string(),
            "[ToolEnd] ✓ list (4ms)".to_string(),
            "  └─ Cargo.toml".to_string(),
        ];
        assert_eq!(collapsed_lines, collapsed_expected);

        // Verbose + details_only: steps shown, collapsed card hint
        let mut details_only = ReplVisualizationState::new(true);
        details_only.show_tool_details = true;
        details_only.expand_tool_cards = false;
        details_only.verbosity = DisplayVerbosity::Verbose;
        let details_lines = render_event_snapshot(&events, &mut details_only);
        let details_expected = vec![
            "[RoundSep] ── Round 1 ──".to_string(),
            "[Thinking][r1]".to_string(),
            "  └─ analyze project structure".to_string(),
            "[Step][r1] llm_round_1_start".to_string(),
            "[Tool][r1] done list (4ms)".to_string(),
            "  ├─ output: Cargo.toml".to_string(),
            "  └─ (collapsed card, use /cards or Ctrl+E)".to_string(),
        ];
        assert_eq!(details_lines, details_expected);

        // Verbose + expanded: full detail with meta
        let mut expanded = ReplVisualizationState::new(true);
        expanded.show_tool_details = true;
        expanded.expand_tool_cards = true;
        expanded.verbosity = DisplayVerbosity::Verbose;
        let expanded_lines = render_event_snapshot(&events, &mut expanded);
        let expanded_expected = vec![
            "[RoundSep] ── Round 1 ──".to_string(),
            "[Thinking][r1]".to_string(),
            "  └─ analyze project structure".to_string(),
            "[Step][r1] llm_round_1_start".to_string(),
            "[Tool][r1] done list (4ms)".to_string(),
            "  ├─ output: Cargo.toml".to_string(),
            "  ├─ input : {\"path\":\".\"}".to_string(),
            "  └─ meta  : call_id=call-9 status=ok".to_string(),
        ];
        assert_eq!(expanded_lines, expanded_expected);

        let mut entries: Vec<ChatEntry> = Vec::new();
        expanded.timeline_cache = vec![mk_event(
            ndc_core::AgentExecutionEventKind::StepFinish,
            "llm_round_1_finish",
            1,
            None,
            None,
            Some(9),
            false,
        )];
        expanded.timeline_limit = 10;
        append_recent_timeline(&mut entries, &expanded);
        let joined = entries_to_plain_text(&entries);
        assert!(joined.contains("Recent Execution Timeline"));
        assert!(joined.contains("llm_round_1_finish"));
    }

    #[test]
    fn test_permission_message_set_and_cleared() {
        let mut viz_state = ReplVisualizationState::new(false);
        assert!(!viz_state.permission_blocked);
        assert!(viz_state.permission_pending_message.is_none());

        // PermissionAsked should set both
        let perm_event = mk_event(
            ndc_core::AgentExecutionEventKind::PermissionAsked,
            "Allow file write?",
            1,
            None,
            None,
            None,
            false,
        );
        event_to_lines(&perm_event, &mut viz_state);
        assert!(viz_state.permission_blocked);
        assert_eq!(
            viz_state.permission_pending_message.as_deref(),
            Some("Allow file write?")
        );

        // A non-permission event should clear both
        let step_event = mk_event(
            ndc_core::AgentExecutionEventKind::StepStart,
            "step",
            1,
            None,
            None,
            None,
            false,
        );
        event_to_lines(&step_event, &mut viz_state);
        assert!(!viz_state.permission_blocked);
        assert!(viz_state.permission_pending_message.is_none());
    }

    #[test]
    fn test_verbosity_compact_hides_steps() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::StepStart,
            "llm_round_1_start",
            1,
            None,
            None,
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert!(lines.is_empty(), "Compact mode should hide StepStart");
    }

    #[test]
    fn test_verbosity_compact_hides_token_usage() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::TokenUsage,
            "token_usage: source=provider prompt=10 completion=5 total=15 | session_prompt_total=22 session_completion_total=11 session_total=33",
            1,
            None,
            None,
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert!(lines.is_empty(), "Compact mode should hide TokenUsage");
        // But state should still be updated
        assert_eq!(viz.latest_round_token_total, 15);
        assert_eq!(viz.session_token_total, 33);
    }

    #[test]
    fn test_verbosity_normal_shows_token_usage() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::TokenUsage,
            "token_usage: source=provider prompt=10 completion=5 total=15 | session_prompt_total=22 session_completion_total=11 session_total=33",
            1,
            None,
            None,
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert!(lines.iter().any(|l| l.contains("[Usage]")));
        assert!(lines.iter().any(|l| l.contains("total")));
    }

    #[test]
    fn test_verbosity_compact_stage_single_line() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "stage changed".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Planning),
            workflow_detail: Some("building context".to_string()),
            workflow_stage_index: Some(1),
            workflow_stage_total: Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES),
        };
        let lines = event_to_lines(&event, &mut viz);
        assert_eq!(
            lines.len(),
            1,
            "Compact should produce exactly 1 line for stage"
        );
        assert!(lines[0].contains("[Stage]"));
        assert!(lines[0].contains("Planning"));
        assert!(lines[0].ends_with("..."));
    }

    #[test]
    fn test_verbosity_normal_stage_with_detail() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "stage changed".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Analysis),
            workflow_detail: Some("scanning files".to_string()),
            workflow_stage_index: Some(3),
            workflow_stage_total: Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES),
        };
        let lines = event_to_lines(&event, &mut viz);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Analysis") && l.contains("scanning files"))
        );
    }

    #[test]
    fn test_round_separator_normal_mode() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallStart,
            "tool_call_start: read | args_preview: {\"path\":\".\"}",
            2,
            Some("read"),
            Some("call-1"),
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("[RoundSep]") && l.contains("Round 2"))
        );
    }

    #[test]
    fn test_round_separator_compact_hidden() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallStart,
            "tool_call_start: read | args_preview: {\"path\":\".\"}",
            2,
            Some("read"),
            Some("call-1"),
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert!(
            !lines.iter().any(|l| l.contains("[RoundSep]")),
            "Compact should not show round separators"
        );
    }

    #[test]
    fn test_round_separator_not_duplicated() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        // First event in round 3
        let event1 = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallStart,
            "tool_call_start: read | args_preview: {\"path\":\".\"}",
            3,
            Some("read"),
            Some("call-1"),
            None,
            false,
        );
        let lines1 = event_to_lines(&event1, &mut viz);
        assert!(lines1.iter().any(|l| l.contains("[RoundSep]")));

        // Second event in same round 3 — no separator
        let event2 = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallEnd,
            "tool_call_end: read (ok) | result_preview: ok",
            3,
            Some("read"),
            Some("call-1"),
            Some(5),
            false,
        );
        let lines2 = event_to_lines(&event2, &mut viz);
        assert!(
            !lines2.iter().any(|l| l.contains("[RoundSep]")),
            "Same round should not repeat separator"
        );
    }

    #[test]
    fn test_permission_compact_shows_hint() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::PermissionAsked,
            "Command not allowed: rm -rf /",
            1,
            None,
            None,
            None,
            true,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert!(lines.iter().any(|l| l.contains("[PermBlock]")));
        assert!(lines.iter().any(|l| l.contains("[PermHint]")));
        assert!(viz.permission_blocked);
    }

    #[test]
    fn test_compact_tool_call_summary_single_line() {
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::ToolCallStart,
            timestamp: chrono::Utc::now(),
            message: "tool_call_start: shell | args_preview: {\"command\":\"cargo build\"}"
                .to_string(),
            round: 1,
            tool_name: Some("shell".to_string()),
            tool_call_id: Some("call-1".to_string()),
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let lines = event_to_lines(&event, &mut viz);
        assert_eq!(
            lines.len(),
            1,
            "Compact ToolCallStart should be single line"
        );
        assert!(lines[0].contains("[ToolRun]"));
        assert!(lines[0].contains("shell"));
        assert!(lines[0].contains("cargo build"));
    }
}
