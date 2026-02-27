//! Chat Renderer — structured chat entry model and rendering.
//!
//! Extracted from `repl.rs` (SEC-S1 God Object refactoring).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::redaction::sanitize_text;

use super::{
    DisplayVerbosity, ReplVisualizationState, TIMELINE_CACHE_MAX_EVENTS, TuiSessionViewState,
    append_timeline_events, capitalize_stage, extract_tool_args_preview,
    extract_tool_result_preview, extract_tool_summary, format_duration_ms, format_token_count,
    truncate_output,
};

use ndc_core::{AgentExecutionEvent, AgentExecutionEventKind, AgentSessionExecutionEvent};

#[derive(Debug, Clone, Copy)]
pub(crate) struct TuiTheme {
    pub(crate) text_strong: Color,
    pub(crate) text_base: Color,
    pub(crate) text_muted: Color,
    pub(crate) text_dim: Color,
    pub(crate) primary: Color,
    pub(crate) success: Color,
    pub(crate) warning: Color,
    pub(crate) danger: Color,
    pub(crate) info: Color,
    pub(crate) user_accent: Color,
    pub(crate) assistant_accent: Color,
    pub(crate) tool_accent: Color,
    pub(crate) thinking_accent: Color,
    pub(crate) border_normal: Color,
    pub(crate) border_active: Color,
    pub(crate) border_dim: Color,
    pub(crate) progress_done: Color,
    pub(crate) progress_active: Color,
    pub(crate) progress_pending: Color,
}

impl TuiTheme {
    pub(crate) fn default_dark() -> Self {
        Self {
            text_strong: Color::White,
            text_base: Color::Gray,
            text_muted: Color::DarkGray,
            text_dim: Color::Rgb(100, 100, 100),
            primary: Color::Cyan,
            success: Color::Green,
            warning: Color::Yellow,
            danger: Color::Red,
            info: Color::Blue,
            user_accent: Color::Blue,
            assistant_accent: Color::Cyan,
            tool_accent: Color::Gray,
            thinking_accent: Color::Magenta,
            border_normal: Color::DarkGray,
            border_active: Color::Cyan,
            border_dim: Color::Rgb(60, 60, 60),
            progress_done: Color::Green,
            progress_active: Color::Cyan,
            progress_pending: Color::DarkGray,
        }
    }
}

/// Status of a tool call card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolCardStatus {
    Running,
    Completed,
    Failed,
}

/// A collapsible card representing a tool call execution.
#[derive(Debug, Clone)]
pub(crate) struct ToolCallCard {
    pub(crate) name: String,
    pub(crate) status: ToolCardStatus,
    pub(crate) duration: Option<String>,
    pub(crate) args_summary: Option<String>,
    pub(crate) output_preview: Option<String>,
    pub(crate) is_error: bool,
    pub(crate) collapsed: bool,
}

/// A single structured entry in the conversation log.
#[derive(Debug, Clone)]
pub(crate) enum ChatEntry {
    /// Visual separator (blank line)
    Separator,
    /// User input message with turn identifier
    UserMessage { content: String, turn_id: usize },
    /// Assistant response with turn identifier
    AssistantMessage { content: String, turn_id: usize },
    /// System/agent note (e.g., processing indicator)
    SystemNote(String),
    /// Round separator divider
    RoundSeparator { round: usize },
    /// Tool call card (collapsible)
    ToolCard(ToolCallCard),
    /// Reasoning block (collapsible, default collapsed)
    ReasoningBlock {
        round: usize,
        content: String,
        collapsed: bool,
    },
    /// Workflow stage indicator
    StageNote(String),
    /// Token usage information
    UsageNote(String),
    /// Error message
    ErrorNote(String),
    /// Warning message
    WarningNote(String),
    /// Permission request
    PermissionNote(String),
    /// Permission hint
    PermissionHint(String),
}

pub(crate) const TUI_MAX_CHAT_ENTRIES: usize = 3000;

pub(crate) fn push_chat_entry(entries: &mut Vec<ChatEntry>, entry: ChatEntry) {
    entries.push(entry);
    if entries.len() > TUI_MAX_CHAT_ENTRIES {
        let overflow = entries.len() - TUI_MAX_CHAT_ENTRIES;
        entries.drain(0..overflow);
    }
}

pub(crate) fn push_chat_entries(entries: &mut Vec<ChatEntry>, new_entries: Vec<ChatEntry>) {
    for entry in new_entries {
        push_chat_entry(entries, entry);
    }
}

/// Count the number of rendered display lines a single ChatEntry will produce.
pub(crate) fn chat_entry_display_lines(entry: &ChatEntry) -> usize {
    match entry {
        ChatEntry::Separator => 1,
        ChatEntry::UserMessage { content, .. } => {
            // header + content lines + footer
            2 + content.lines().count().max(1)
        }
        ChatEntry::AssistantMessage { content, .. } => 2 + content.lines().count().max(1),
        ChatEntry::SystemNote(_)
        | ChatEntry::RoundSeparator { .. }
        | ChatEntry::StageNote(_)
        | ChatEntry::UsageNote(_)
        | ChatEntry::ErrorNote(_)
        | ChatEntry::WarningNote(_)
        | ChatEntry::PermissionNote(_)
        | ChatEntry::PermissionHint(_) => 1,
        ChatEntry::ToolCard(card) => {
            let mut n = 1; // header line
            if !card.collapsed {
                if card.args_summary.is_some() {
                    n += 1;
                }
                if card.output_preview.is_some() {
                    n += 1;
                }
            }
            n
        }
        ChatEntry::ReasoningBlock {
            collapsed, content, ..
        } => {
            if *collapsed {
                1
            } else {
                1 + content.lines().count().max(1)
            }
        }
    }
}

/// Total rendered display lines for a slice of entries.
pub(crate) fn total_display_lines(entries: &[ChatEntry]) -> usize {
    entries.iter().map(chat_entry_display_lines).sum()
}

/// Render structured chat entries to styled ratatui Lines.
pub(crate) fn style_chat_entries(entries: &[ChatEntry]) -> Vec<Line<'static>> {
    let theme = TuiTheme::default_dark();
    let mut lines = Vec::new();
    for entry in entries {
        style_chat_entry(entry, &theme, &mut lines);
    }
    lines
}

/// Render a single ChatEntry into styled Lines.
pub(crate) fn style_chat_entry(
    entry: &ChatEntry,
    theme: &TuiTheme,
    lines: &mut Vec<Line<'static>>,
) {
    match entry {
        ChatEntry::Separator => {
            lines.push(Line::default());
        }
        ChatEntry::UserMessage { content, turn_id } => {
            // Header: ▌ You [#n]
            lines.push(Line::from(vec![
                Span::styled("▌ ", Style::default().fg(theme.user_accent)),
                Span::styled(
                    format!("You [#{}]", turn_id),
                    Style::default()
                        .fg(theme.user_accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            // Content with left border
            for l in content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.user_accent)),
                    Span::styled(l.to_string(), Style::default().fg(theme.text_strong)),
                ]));
            }
            if content.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.user_accent)),
                    Span::styled("", Style::default()),
                ]));
            }
            // Footer
            lines.push(Line::from(Span::styled(
                "└─",
                Style::default().fg(theme.user_accent),
            )));
        }
        ChatEntry::AssistantMessage { content, turn_id } => {
            lines.push(Line::from(vec![
                Span::styled("▌ ", Style::default().fg(theme.assistant_accent)),
                Span::styled(
                    format!("Assistant [#{}]", turn_id),
                    Style::default()
                        .fg(theme.assistant_accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            for l in content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.assistant_accent)),
                    Span::styled(l.to_string(), Style::default().fg(theme.text_base)),
                ]));
            }
            if content.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(theme.assistant_accent)),
                    Span::styled("", Style::default()),
                ]));
            }
            lines.push(Line::from(Span::styled(
                "└─",
                Style::default().fg(theme.assistant_accent),
            )));
        }
        ChatEntry::SystemNote(text) => {
            lines.push(Line::from(vec![
                Span::styled("  ◆ ", Style::default().fg(theme.warning)),
                Span::styled(text.clone(), Style::default().fg(theme.warning)),
            ]));
        }
        ChatEntry::RoundSeparator { round } => {
            lines.push(Line::from(Span::styled(
                format!("  ── Round {} ──", round),
                Style::default()
                    .fg(theme.text_dim)
                    .add_modifier(Modifier::DIM),
            )));
        }
        ChatEntry::ToolCard(card) => {
            let icon = if card.collapsed { "▸" } else { "▾" };
            let (status_icon, status_color) = match card.status {
                ToolCardStatus::Running => ("⟳", theme.warning),
                ToolCardStatus::Completed => ("✓", theme.success),
                ToolCardStatus::Failed => ("✗", theme.danger),
            };
            let dur_str = card
                .duration
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| format!(" ({})", d))
                .unwrap_or_default();
            // Header: ▸/▾ ✓/✗/⟳ tool_name (duration)
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", icon),
                    Style::default().fg(theme.tool_accent),
                ),
                Span::styled(status_icon.to_string(), Style::default().fg(status_color)),
                Span::styled(
                    format!(" {}", card.name),
                    Style::default().fg(theme.text_strong),
                ),
                Span::styled(dur_str, Style::default().fg(theme.text_muted)),
            ]));
            if !card.collapsed {
                if let Some(args) = &card.args_summary {
                    lines.push(Line::from(vec![
                        Span::styled("    ├─ input : ", Style::default().fg(theme.text_muted)),
                        Span::styled(args.clone(), Style::default().fg(theme.text_base)),
                    ]));
                }
                if let Some(output) = &card.output_preview {
                    let prefix = if card.is_error { "error " } else { "output" };
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("    └─ {}: ", prefix),
                            Style::default().fg(theme.text_muted),
                        ),
                        Span::styled(output.clone(), Style::default().fg(theme.text_base)),
                    ]));
                }
            }
        }
        ChatEntry::ReasoningBlock {
            round,
            content,
            collapsed,
        } => {
            if *collapsed {
                lines.push(Line::from(vec![
                    Span::styled("  ▸ ", Style::default().fg(theme.thinking_accent)),
                    Span::styled(
                        format!("Thinking [r{}] (collapsed)", round),
                        Style::default().fg(theme.thinking_accent),
                    ),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("  ▾ ", Style::default().fg(theme.thinking_accent)),
                    Span::styled(
                        format!("Thinking [r{}]", round),
                        Style::default()
                            .fg(theme.thinking_accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                for l in content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(l.to_string(), Style::default().fg(theme.text_muted)),
                    ]));
                }
                if content.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled("", Style::default()),
                    ]));
                }
            }
        }
        ChatEntry::StageNote(text) => {
            lines.push(Line::from(vec![
                Span::styled("  ◆ ", Style::default().fg(theme.primary)),
                Span::styled(text.clone(), Style::default().fg(theme.primary)),
            ]));
        }
        ChatEntry::UsageNote(text) => {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(text.clone(), Style::default().fg(theme.text_muted)),
            ]));
        }
        ChatEntry::ErrorNote(text) => {
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✗ ",
                    Style::default()
                        .fg(theme.danger)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(text.clone(), Style::default().fg(theme.danger)),
            ]));
        }
        ChatEntry::WarningNote(text) => {
            lines.push(Line::from(vec![
                Span::styled(
                    "  ⚠ ",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(text.clone(), Style::default().fg(theme.warning)),
            ]));
        }
        ChatEntry::PermissionNote(text) => {
            lines.push(Line::from(vec![
                Span::styled(
                    "  ⚠ ",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(text.clone(), Style::default().fg(theme.warning)),
            ]));
        }
        ChatEntry::PermissionHint(text) => {
            lines.push(Line::from(vec![
                Span::styled("  ⓘ ", Style::default().fg(theme.info)),
                Span::styled(text.clone(), Style::default().fg(theme.info)),
            ]));
        }
    }
}

/// Convert an AgentExecutionEvent into structured ChatEntry variants.
pub(crate) fn event_to_entries(
    event: &AgentExecutionEvent,
    viz_state: &mut ReplVisualizationState,
) -> Vec<ChatEntry> {
    if !matches!(
        event.kind,
        AgentExecutionEventKind::PermissionAsked | AgentExecutionEventKind::Reasoning
    ) {
        viz_state.permission_blocked = false;
        viz_state.permission_pending_message = None;
    }
    let v = viz_state.verbosity;
    let mut entries = Vec::new();

    // Round separator (Normal/Verbose only)
    if matches!(v, DisplayVerbosity::Normal | DisplayVerbosity::Verbose)
        && event.round > viz_state.last_emitted_round
        && event.round > 0
    {
        entries.push(ChatEntry::RoundSeparator { round: event.round });
    }
    if event.round > 0 {
        viz_state.last_emitted_round = event.round;
    }

    match event.kind {
        AgentExecutionEventKind::WorkflowStage => {
            if let Some(stage_info) = event.workflow_stage_info() {
                let stage = stage_info.stage;
                viz_state.current_workflow_stage = Some(stage.as_str().to_string());
                viz_state.current_workflow_stage_index = Some(stage_info.index);
                viz_state.current_workflow_stage_total = Some(stage_info.total);
                viz_state.current_workflow_stage_started_at = Some(event.timestamp);
                match v {
                    DisplayVerbosity::Compact => {
                        entries.push(ChatEntry::StageNote(format!(
                            "{}...",
                            capitalize_stage(stage.as_str())
                        )));
                    }
                    DisplayVerbosity::Normal => {
                        let detail = if stage_info.detail.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", stage_info.detail)
                        };
                        entries.push(ChatEntry::StageNote(format!(
                            "{}{}",
                            capitalize_stage(stage.as_str()),
                            detail
                        )));
                    }
                    DisplayVerbosity::Verbose => {
                        entries.push(ChatEntry::StageNote(format!("stage:{}", stage)));
                        entries.push(ChatEntry::SystemNote(format!(
                            "[Workflow][r{}] {}",
                            event.round,
                            sanitize_text(&event.message, viz_state.redaction_mode)
                        )));
                    }
                }
            } else {
                entries.push(ChatEntry::SystemNote(format!(
                    "[Workflow][r{}] {}",
                    event.round,
                    sanitize_text(&event.message, viz_state.redaction_mode)
                )));
            }
        }
        AgentExecutionEventKind::Reasoning => {
            if viz_state.show_thinking {
                entries.push(ChatEntry::ReasoningBlock {
                    round: event.round,
                    content: sanitize_text(&event.message, viz_state.redaction_mode),
                    collapsed: false,
                });
            } else if !viz_state.hidden_thinking_round_hints.contains(&event.round) {
                viz_state.hidden_thinking_round_hints.insert(event.round);
                entries.push(ChatEntry::ReasoningBlock {
                    round: event.round,
                    content: sanitize_text(&event.message, viz_state.redaction_mode),
                    collapsed: true,
                });
            }
        }
        AgentExecutionEventKind::ToolCallStart => {
            let tool = event.tool_name.as_deref().unwrap_or("unknown");
            let args = extract_tool_args_preview(&event.message)
                .map(|a| sanitize_text(a, viz_state.redaction_mode));
            let args_summary = match v {
                DisplayVerbosity::Compact => args.as_deref().and_then(|a| {
                    let summary = extract_tool_summary(tool, a);
                    if summary.is_empty() {
                        None
                    } else {
                        let (s, _) = truncate_output(&summary, 80);
                        Some(s)
                    }
                }),
                DisplayVerbosity::Normal => args.as_deref().and_then(|a| {
                    let summary = extract_tool_summary(tool, a);
                    if summary.is_empty() {
                        None
                    } else {
                        Some(summary)
                    }
                }),
                DisplayVerbosity::Verbose => args.clone(),
            };
            entries.push(ChatEntry::ToolCard(ToolCallCard {
                name: tool.to_string(),
                status: ToolCardStatus::Running,
                duration: None,
                args_summary,
                output_preview: None,
                is_error: false,
                collapsed: !viz_state.expand_tool_cards,
            }));
        }
        AgentExecutionEventKind::ToolCallEnd => {
            let tool = event.tool_name.as_deref().unwrap_or("unknown");
            let duration = event.duration_ms.map(format_duration_ms);
            let output = extract_tool_result_preview(&event.message)
                .map(|p| sanitize_text(p, viz_state.redaction_mode));
            let output_preview = match v {
                DisplayVerbosity::Compact => output.map(|o| {
                    let (msg, truncated) = truncate_output(&o, 100);
                    if truncated {
                        format!("{} …", msg)
                    } else {
                        msg
                    }
                }),
                DisplayVerbosity::Normal | DisplayVerbosity::Verbose => output,
            };
            entries.push(ChatEntry::ToolCard(ToolCallCard {
                name: tool.to_string(),
                status: if event.is_error {
                    ToolCardStatus::Failed
                } else {
                    ToolCardStatus::Completed
                },
                duration,
                args_summary: if viz_state.expand_tool_cards
                    || matches!(v, DisplayVerbosity::Verbose)
                {
                    extract_tool_args_preview(&event.message)
                        .map(|a| sanitize_text(a, viz_state.redaction_mode))
                } else {
                    None
                },
                output_preview,
                is_error: event.is_error,
                collapsed: !viz_state.expand_tool_cards,
            }));
        }
        AgentExecutionEventKind::TokenUsage => {
            if let Some(usage) = event.token_usage_info() {
                viz_state.latest_round_token_total = usage.total_tokens;
                viz_state.session_token_total = usage.session_total;
            }
            match v {
                DisplayVerbosity::Compact => {}
                DisplayVerbosity::Normal => {
                    if let Some(usage) = event.token_usage_info() {
                        entries.push(ChatEntry::UsageNote(format!(
                            "tok +{} ({} total)",
                            format_token_count(usage.total_tokens),
                            format_token_count(usage.session_total),
                        )));
                    }
                }
                DisplayVerbosity::Verbose => {
                    entries.push(ChatEntry::UsageNote(format!(
                        "[Usage][r{}] {}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode)
                    )));
                }
            }
        }
        AgentExecutionEventKind::PermissionAsked => {
            viz_state.permission_blocked = true;
            viz_state.permission_pending_message =
                Some(sanitize_text(&event.message, viz_state.redaction_mode));
            match v {
                DisplayVerbosity::Compact | DisplayVerbosity::Normal => {
                    let msg = sanitize_text(&event.message, viz_state.redaction_mode);
                    entries.push(ChatEntry::PermissionNote(msg));
                    entries.push(ChatEntry::PermissionHint(
                        "Reply in terminal to approve, or set /allow".to_string(),
                    ));
                }
                DisplayVerbosity::Verbose => {
                    entries.push(ChatEntry::PermissionNote(format!(
                        "[Permission][r{}] {}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode)
                    )));
                    entries.push(ChatEntry::PermissionHint(
                        "Reply in terminal to approve, or set /allow".to_string(),
                    ));
                }
            }
        }
        AgentExecutionEventKind::StepStart
        | AgentExecutionEventKind::StepFinish
        | AgentExecutionEventKind::Verification => match v {
            DisplayVerbosity::Compact => {}
            DisplayVerbosity::Normal => {
                if matches!(event.kind, AgentExecutionEventKind::StepFinish)
                    && event.duration_ms.is_some()
                {
                    entries.push(ChatEntry::SystemNote(format!(
                        "[Step][r{}] {}{}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode),
                        event
                            .duration_ms
                            .map(|d| format!(" ({})", format_duration_ms(d)))
                            .unwrap_or_default()
                    )));
                }
            }
            DisplayVerbosity::Verbose => {
                if !viz_state.show_tool_details
                    && matches!(event.kind, AgentExecutionEventKind::StepStart)
                {
                    entries.push(ChatEntry::SystemNote(format!(
                        "[Agent][r{}] thinking...",
                        event.round
                    )));
                } else if viz_state.show_tool_details {
                    entries.push(ChatEntry::SystemNote(format!(
                        "[Step][r{}] {}{}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode),
                        event
                            .duration_ms
                            .map(|d| format!(" ({}ms)", d))
                            .unwrap_or_default()
                    )));
                }
            }
        },
        AgentExecutionEventKind::Error => {
            entries.push(ChatEntry::ErrorNote(format!(
                "[Error][r{}] {}",
                event.round,
                sanitize_text(&event.message, viz_state.redaction_mode)
            )));
        }
        AgentExecutionEventKind::SessionStatus | AgentExecutionEventKind::Text => {}
    }
    entries
}

/// Drain live execution events into structured chat entries.
pub(crate) fn drain_live_chat_entries(
    receiver: &mut Option<tokio::sync::broadcast::Receiver<AgentSessionExecutionEvent>>,
    expected_session_id: Option<&str>,
    viz_state: &mut ReplVisualizationState,
    entries: &mut Vec<ChatEntry>,
) -> bool {
    let Some(rx) = receiver.as_mut() else {
        return false;
    };
    let mut rendered = false;
    loop {
        match rx.try_recv() {
            Ok(message) => {
                if expected_session_id
                    .map(|sid| sid != message.session_id)
                    .unwrap_or(false)
                {
                    continue;
                }
                append_timeline_events(
                    &mut viz_state.timeline_cache,
                    std::slice::from_ref(&message.event),
                    TIMELINE_CACHE_MAX_EVENTS,
                );
                push_chat_entries(entries, event_to_entries(&message.event, viz_state));
                rendered = true;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                push_chat_entry(
                    entries,
                    ChatEntry::WarningNote(format!(
                        "realtime stream lagged, dropped {} event(s)",
                        skipped
                    )),
                );
                rendered = true;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                *receiver = None;
                push_chat_entry(
                    entries,
                    ChatEntry::WarningNote(
                        "realtime stream closed, fallback to polling".to_string(),
                    ),
                );
                rendered = true;
                break;
            }
        }
    }
    rendered
}

/// Compute effective scroll offset for chat entries (display-line based).
pub(crate) fn effective_chat_scroll(entries: &[ChatEntry], view: &TuiSessionViewState) -> usize {
    let total = total_display_lines(entries);
    if view.auto_follow || total <= view.body_height {
        total.saturating_sub(view.body_height)
    } else {
        view.scroll_offset
            .min(total.saturating_sub(view.body_height))
    }
}

/// Toggle collapse state for all tool cards in entries.
pub(crate) fn toggle_all_tool_cards(entries: &mut [ChatEntry]) {
    for entry in entries.iter_mut() {
        if let ChatEntry::ToolCard(card) = entry {
            card.collapsed = !card.collapsed;
        }
    }
}

/// Toggle collapse state for all reasoning blocks in entries.
pub(crate) fn toggle_all_reasoning_blocks(entries: &mut [ChatEntry]) {
    for entry in entries.iter_mut() {
        if let ChatEntry::ReasoningBlock { collapsed, .. } = entry {
            *collapsed = !*collapsed;
        }
    }
}

/// Bridge function: push a plain text string as a typed ChatEntry.
/// Empty text becomes Separator; "[Error]" prefix becomes ErrorNote;
/// "[Warning]" or "[Tip]" becomes WarningNote; everything else SystemNote.
pub(crate) fn push_text_entry(entries: &mut Vec<ChatEntry>, text: &str) {
    if text.is_empty() {
        push_chat_entry(entries, ChatEntry::Separator);
    } else if text.starts_with("[Error]") {
        push_chat_entry(entries, ChatEntry::ErrorNote(text.to_string()));
    } else if text.starts_with("[Warning]") || text.starts_with("[Tip]") {
        push_chat_entry(entries, ChatEntry::WarningNote(text.to_string()));
    } else {
        push_chat_entry(entries, ChatEntry::SystemNote(text.to_string()));
    }
}

/// Convert ChatEntry list to plain text for export (/copy command).
pub(crate) fn entries_to_plain_text(entries: &[ChatEntry]) -> String {
    let mut lines = Vec::new();
    for entry in entries {
        match entry {
            ChatEntry::Separator => lines.push(String::new()),
            ChatEntry::UserMessage { content, turn_id } => {
                lines.push(format!("You [#{}]: {}", turn_id, content));
            }
            ChatEntry::AssistantMessage { content, turn_id } => {
                lines.push(format!("Assistant [#{}]:", turn_id));
                for l in content.lines() {
                    lines.push(format!("  {}", l));
                }
            }
            ChatEntry::SystemNote(text)
            | ChatEntry::StageNote(text)
            | ChatEntry::UsageNote(text)
            | ChatEntry::ErrorNote(text)
            | ChatEntry::WarningNote(text)
            | ChatEntry::PermissionNote(text)
            | ChatEntry::PermissionHint(text) => {
                lines.push(text.clone());
            }
            ChatEntry::RoundSeparator { round } => {
                lines.push(format!("── Round {} ──", round));
            }
            ChatEntry::ToolCard(card) => {
                let icon = match card.status {
                    ToolCardStatus::Running => "⟳",
                    ToolCardStatus::Completed => "✓",
                    ToolCardStatus::Failed => "✗",
                };
                let dur = card
                    .duration
                    .as_deref()
                    .map(|d| format!(" ({})", d))
                    .unwrap_or_default();
                lines.push(format!("{} {}{}", icon, card.name, dur));
                if !card.collapsed {
                    if let Some(args) = &card.args_summary {
                        lines.push(format!("  input: {}", args));
                    }
                    if let Some(output) = &card.output_preview {
                        lines.push(format!("  output: {}", output));
                    }
                }
            }
            ChatEntry::ReasoningBlock {
                round,
                content,
                collapsed,
            } => {
                if *collapsed {
                    lines.push(format!("Thinking [r{}] (collapsed)", round));
                } else {
                    lines.push(format!("Thinking [r{}]:", round));
                    for l in content.lines() {
                        lines.push(format!("  {}", l));
                    }
                }
            }
        }
    }
    lines.join("\n")
}
