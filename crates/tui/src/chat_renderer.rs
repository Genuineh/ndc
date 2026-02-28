//! Chat Renderer — structured chat entry model and rendering.
//!
//! Extracted from `repl.rs` (SEC-S1 God Object refactoring).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use ndc_core::redaction::sanitize_text;

use super::{
    DisplayVerbosity, ReplVisualizationState, TIMELINE_CACHE_MAX_EVENTS, TuiSessionViewState,
    append_timeline_events, capitalize_stage, extract_tool_args_preview,
    extract_tool_result_preview, extract_tool_summary, format_duration_ms, format_token_count,
    truncate_output,
};

use ndc_core::{AgentExecutionEvent, AgentExecutionEventKind, AgentSessionExecutionEvent};

#[derive(Debug, Clone, Copy)]
pub struct TuiTheme {
    pub text_strong: Color,
    pub text_base: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub primary: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub info: Color,
    pub user_accent: Color,
    pub assistant_accent: Color,
    pub tool_accent: Color,
    pub thinking_accent: Color,
    pub border_normal: Color,
    pub border_active: Color,
    pub border_dim: Color,
    pub progress_done: Color,
    pub progress_active: Color,
    pub progress_pending: Color,
}

impl TuiTheme {
    pub fn default_dark() -> Self {
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
pub enum ToolCardStatus {
    Running,
    Completed,
    Failed,
}

/// A collapsible card representing a tool call execution.
#[derive(Debug, Clone)]
pub struct ToolCallCard {
    pub name: String,
    pub status: ToolCardStatus,
    pub duration: Option<String>,
    pub args_summary: Option<String>,
    pub output_preview: Option<String>,
    pub is_error: bool,
    pub collapsed: bool,
}

/// A single line in a diff preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    /// Added line (rendered green with `+` prefix)
    Added(String),
    /// Removed line (rendered red with `-` prefix)
    Removed(String),
    /// Unchanged context line (rendered dimmed)
    Context(String),
}

/// A single structured entry in the conversation log.
#[derive(Debug, Clone)]
pub enum ChatEntry {
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
    /// Inline diff preview for write/edit tool results
    DiffPreview {
        path: String,
        lines: Vec<DiffLine>,
        collapsed: bool,
    },
}

pub const TUI_MAX_CHAT_ENTRIES: usize = 3000;

pub fn push_chat_entry(entries: &mut Vec<ChatEntry>, entry: ChatEntry) {
    entries.push(entry);
    if entries.len() > TUI_MAX_CHAT_ENTRIES {
        let overflow = entries.len() - TUI_MAX_CHAT_ENTRIES;
        entries.drain(0..overflow);
    }
}

pub fn push_chat_entries(entries: &mut Vec<ChatEntry>, new_entries: Vec<ChatEntry>) {
    for entry in new_entries {
        push_chat_entry(entries, entry);
    }
}

/// Count the number of rendered display lines a single ChatEntry will produce.
pub fn chat_entry_display_lines(entry: &ChatEntry) -> usize {
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
        ChatEntry::DiffPreview {
            collapsed, lines, ..
        } => {
            if *collapsed {
                1 // header only
            } else {
                1 + lines.len() // header + diff lines
            }
        }
    }
}

/// Total rendered display lines for a slice of entries.
pub fn total_display_lines(entries: &[ChatEntry]) -> usize {
    entries.iter().map(chat_entry_display_lines).sum()
}

/// Render structured chat entries to styled ratatui Lines.
pub fn style_chat_entries(entries: &[ChatEntry]) -> Vec<Line<'static>> {
    let theme = TuiTheme::default_dark();
    let mut lines = Vec::new();
    for entry in entries {
        style_chat_entry(entry, &theme, &mut lines);
    }
    lines
}

/// Render a single ChatEntry into styled Lines.
pub fn style_chat_entry(entry: &ChatEntry, theme: &TuiTheme, lines: &mut Vec<Line<'static>>) {
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
            // Tool-type accent color: write→green, shell→orange, default→tool_accent
            let tool_color = tool_type_accent(&card.name, theme);
            let dur_str = card
                .duration
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| format!(" ({})", d))
                .unwrap_or_default();
            // Header: ▸/▾ ✓/✗/⟳ tool_name (duration)
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), Style::default().fg(tool_color)),
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
        ChatEntry::DiffPreview {
            path,
            lines: diff_lines,
            collapsed,
        } => {
            let icon = if *collapsed { "▸" } else { "▾" };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), Style::default().fg(theme.info)),
                Span::styled("Diff: ", Style::default().fg(theme.info)),
                Span::styled(
                    path.clone(),
                    Style::default()
                        .fg(theme.text_strong)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            if !*collapsed {
                for dl in diff_lines {
                    match dl {
                        DiffLine::Added(text) => {
                            lines.push(Line::from(Span::styled(
                                format!("    + {}", text),
                                Style::default().fg(theme.success),
                            )));
                        }
                        DiffLine::Removed(text) => {
                            lines.push(Line::from(Span::styled(
                                format!("    - {}", text),
                                Style::default().fg(theme.danger),
                            )));
                        }
                        DiffLine::Context(text) => {
                            lines.push(Line::from(Span::styled(
                                format!("      {}", text),
                                Style::default().fg(theme.text_dim),
                            )));
                        }
                    }
                }
            }
        }
    }
}

/// Convert an AgentExecutionEvent into structured ChatEntry variants.
pub fn event_to_entries(
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
            // Generate DiffPreview for write/edit tool completions
            if !event.is_error
                && is_write_tool_name(tool)
                && let Some(preview) = build_diff_preview(&event.message, viz_state.redaction_mode)
            {
                entries.push(preview);
            }
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
        AgentExecutionEventKind::SessionStatus
        | AgentExecutionEventKind::Text
        | AgentExecutionEventKind::TodoStateChange
        | AgentExecutionEventKind::AnalysisComplete
        | AgentExecutionEventKind::PlanningComplete
        | AgentExecutionEventKind::TodoExecutionStart
        | AgentExecutionEventKind::TodoExecutionEnd
        | AgentExecutionEventKind::Report => {}
    }
    entries
}

/// Drain live execution events into structured chat entries.
pub fn drain_live_chat_entries(
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
pub fn effective_chat_scroll(entries: &[ChatEntry], view: &TuiSessionViewState) -> usize {
    let total = total_display_lines(entries);
    if view.auto_follow || total <= view.body_height {
        total.saturating_sub(view.body_height)
    } else {
        view.scroll_offset
            .min(total.saturating_sub(view.body_height))
    }
}

/// Toggle collapse state for all tool cards in entries.
pub fn toggle_all_tool_cards(entries: &mut [ChatEntry]) {
    for entry in entries.iter_mut() {
        if let ChatEntry::ToolCard(card) = entry {
            card.collapsed = !card.collapsed;
        }
    }
}

/// Toggle collapse state for all reasoning blocks in entries.
pub fn toggle_all_reasoning_blocks(entries: &mut [ChatEntry]) {
    for entry in entries.iter_mut() {
        if let ChatEntry::ReasoningBlock { collapsed, .. } = entry {
            *collapsed = !*collapsed;
        }
    }
}

/// Bridge function: push a plain text string as a typed ChatEntry.
/// Empty text becomes Separator; "[Error]" prefix becomes ErrorNote;
/// "[Warning]" or "[Tip]" becomes WarningNote; everything else SystemNote.
pub fn push_text_entry(entries: &mut Vec<ChatEntry>, text: &str) {
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

/// Returns an accent color based on the tool type:
/// - write/edit tools → green
/// - shell/exec tools → orange/yellow
/// - other tools → default tool_accent
fn tool_type_accent(tool_name: &str, theme: &TuiTheme) -> Color {
    match tool_name {
        "write_file" | "edit_file" | "create_file" | "patch_file" | "replace_in_file"
        | "insert_code" => theme.success,
        "run_command" | "shell" | "exec" | "bash" | "terminal" | "run_shell" => theme.warning,
        _ => theme.tool_accent,
    }
}

/// Returns `true` for tool names that write or edit files.
fn is_write_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "write_file"
            | "edit_file"
            | "create_file"
            | "patch_file"
            | "replace_in_file"
            | "insert_code"
    )
}

/// Build a DiffPreview entry from a write/edit tool's result message.
///
/// Extracts the file path from `args_preview:` and generates simple diff lines
/// from `result_preview:`. Returns `None` if information is insufficient.
pub fn build_diff_preview(
    message: &str,
    redaction_mode: ndc_core::redaction::RedactionMode,
) -> Option<ChatEntry> {
    let path = extract_diff_path(message)?;
    let result = extract_tool_result_preview(message)?;
    let sanitized = sanitize_text(result, redaction_mode);

    let diff_lines: Vec<DiffLine> = sanitized
        .lines()
        .take(20) // Limit preview to 20 lines
        .map(|line| {
            if let Some(rest) = line.strip_prefix('+') {
                DiffLine::Added(rest.to_string())
            } else if let Some(rest) = line.strip_prefix('-') {
                DiffLine::Removed(rest.to_string())
            } else {
                DiffLine::Context(line.to_string())
            }
        })
        .collect();

    if diff_lines.is_empty() {
        return None;
    }

    Some(ChatEntry::DiffPreview {
        path: sanitize_text(path, redaction_mode),
        lines: diff_lines,
        collapsed: true, // collapsed by default
    })
}

/// Extract file path from a tool message's args_preview JSON.
fn extract_diff_path(message: &str) -> Option<&str> {
    let args = extract_tool_args_preview(message)?;
    // args_preview is typically JSON like {"path":"foo.rs",...}
    // Simple extraction without a JSON parser
    let path_key = "\"path\":\"";
    let start = args.find(path_key)? + path_key.len();
    let rest = &args[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Convert ChatEntry list to plain text for export (/copy command).
pub fn entries_to_plain_text(entries: &[ChatEntry]) -> String {
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
            ChatEntry::DiffPreview {
                path,
                lines: diff_lines,
                collapsed,
            } => {
                if *collapsed {
                    lines.push(format!("Diff: {} (collapsed)", path));
                } else {
                    lines.push(format!("Diff: {}", path));
                    for dl in diff_lines {
                        match dl {
                            DiffLine::Added(text) => lines.push(format!("+ {}", text)),
                            DiffLine::Removed(text) => lines.push(format!("- {}", text)),
                            DiffLine::Context(text) => lines.push(format!("  {}", text)),
                        }
                    }
                }
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[test]
    fn test_build_diff_preview_from_message() {
        use crate::build_diff_preview;
        let msg = r#"tool_call_end: write_file | args_preview: {"path":"src/main.rs"} | result_preview: +new line
-old line
 context"#;
        let preview = build_diff_preview(msg, ndc_core::redaction::RedactionMode::Off);
        assert!(preview.is_some(), "expected DiffPreview");
        if let Some(ChatEntry::DiffPreview {
            path,
            lines,
            collapsed,
        }) = preview
        {
            assert_eq!(path, "src/main.rs");
            assert!(collapsed, "should be collapsed by default");
            assert_eq!(lines.len(), 3);
            assert_eq!(lines[0], DiffLine::Added("new line".to_string()));
            assert_eq!(lines[1], DiffLine::Removed("old line".to_string()));
            assert_eq!(lines[2], DiffLine::Context(" context".to_string()));
        } else {
            panic!("expected DiffPreview variant");
        }
    }

    #[test]
    fn test_build_diff_preview_missing_path_returns_none() {
        use crate::build_diff_preview;
        let msg = "tool_call_end: write_file | result_preview: +added";
        let preview = build_diff_preview(msg, ndc_core::redaction::RedactionMode::Off);
        assert!(preview.is_none(), "no path → no preview");
    }

    #[test]
    fn test_chat_entry_user_message_rendering() {
        let entry = ChatEntry::UserMessage {
            content: "hello world".to_string(),
            turn_id: 1,
        };
        let rendered = entry_lines_plain(&entry);
        assert_eq!(rendered.len(), 3); // header + content + footer
        assert!(rendered[0].contains("You [#1]"));
        assert!(rendered[1].contains("hello world"));
        assert!(rendered[2].contains("└─"));
    }

    #[test]
    fn test_chat_entry_user_message_multiline() {
        let entry = ChatEntry::UserMessage {
            content: "line1\nline2\nline3".to_string(),
            turn_id: 2,
        };
        let rendered = entry_lines_plain(&entry);
        // header + 3 content lines + footer = 5
        assert_eq!(rendered.len(), 5);
        assert!(rendered[0].contains("You [#2]"));
        assert!(rendered[1].contains("line1"));
        assert!(rendered[2].contains("line2"));
        assert!(rendered[3].contains("line3"));
        assert!(rendered[4].contains("└─"));
    }

    #[test]
    fn test_chat_entry_assistant_message_rendering() {
        let entry = ChatEntry::AssistantMessage {
            content: "I can help with that.".to_string(),
            turn_id: 1,
        };
        let rendered = entry_lines_plain(&entry);
        assert_eq!(rendered.len(), 3); // header + content + footer
        assert!(rendered[0].contains("Assistant [#1]"));
        assert!(rendered[1].contains("I can help with that."));
        assert!(rendered[2].contains("└─"));
    }

    #[test]
    fn test_chat_entry_tool_card_collapsed() {
        let card = ToolCallCard {
            name: "shell".to_string(),
            status: ToolCardStatus::Completed,
            duration: Some("1.2s".to_string()),
            args_summary: Some("cargo build".to_string()),
            output_preview: Some("success".to_string()),
            is_error: false,
            collapsed: true,
        };
        let entry = ChatEntry::ToolCard(card);
        let rendered = entry_lines_plain(&entry);
        // Collapsed: only header line
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("▸"));
        assert!(rendered[0].contains("✓"));
        assert!(rendered[0].contains("shell"));
        assert!(rendered[0].contains("(1.2s)"));
    }

    #[test]
    fn test_chat_entry_tool_card_expanded() {
        let card = ToolCallCard {
            name: "shell".to_string(),
            status: ToolCardStatus::Completed,
            duration: Some("1.2s".to_string()),
            args_summary: Some("cargo build".to_string()),
            output_preview: Some("success".to_string()),
            is_error: false,
            collapsed: false,
        };
        let entry = ChatEntry::ToolCard(card);
        let rendered = entry_lines_plain(&entry);
        // Expanded: header + args + output = 3
        assert_eq!(rendered.len(), 3);
        assert!(rendered[0].contains("▾"));
        assert!(rendered[0].contains("✓"));
        assert!(rendered[0].contains("shell"));
        assert!(rendered[1].contains("input"));
        assert!(rendered[1].contains("cargo build"));
        assert!(rendered[2].contains("output"));
        assert!(rendered[2].contains("success"));
    }

    #[test]
    fn test_chat_entry_tool_card_failed() {
        let card = ToolCallCard {
            name: "write".to_string(),
            status: ToolCardStatus::Failed,
            duration: Some("0.5s".to_string()),
            args_summary: None,
            output_preview: Some("permission denied".to_string()),
            is_error: true,
            collapsed: false,
        };
        let entry = ChatEntry::ToolCard(card);
        let rendered = entry_lines_plain(&entry);
        assert!(rendered[0].contains("✗"));
        assert!(rendered[0].contains("write"));
        // Error output should say "error" not "output"
        assert!(
            rendered
                .iter()
                .any(|l| l.contains("error") && l.contains("permission denied"))
        );
    }

    #[test]
    fn test_chat_entry_tool_card_running() {
        let card = ToolCallCard {
            name: "shell".to_string(),
            status: ToolCardStatus::Running,
            duration: None,
            args_summary: Some("cargo test".to_string()),
            output_preview: None,
            is_error: false,
            collapsed: false,
        };
        let entry = ChatEntry::ToolCard(card);
        let rendered = entry_lines_plain(&entry);
        assert!(rendered[0].contains("⟳"));
        assert!(rendered[0].contains("shell"));
    }

    #[test]
    fn test_chat_entry_reasoning_collapsed() {
        let entry = ChatEntry::ReasoningBlock {
            round: 1,
            content: "analyzing the code structure".to_string(),
            collapsed: true,
        };
        let rendered = entry_lines_plain(&entry);
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("▸"));
        assert!(rendered[0].contains("Thinking"));
        assert!(rendered[0].contains("collapsed"));
    }

    #[test]
    fn test_chat_entry_reasoning_expanded() {
        let entry = ChatEntry::ReasoningBlock {
            round: 2,
            content: "step 1: read files\nstep 2: analyze".to_string(),
            collapsed: false,
        };
        let rendered = entry_lines_plain(&entry);
        // header + 2 content lines = 3
        assert_eq!(rendered.len(), 3);
        assert!(rendered[0].contains("▾"));
        assert!(rendered[0].contains("Thinking [r2]"));
        assert!(rendered[1].contains("step 1: read files"));
        assert!(rendered[2].contains("step 2: analyze"));
    }

    #[test]
    fn test_chat_entry_round_separator() {
        let entry = ChatEntry::RoundSeparator { round: 3 };
        let rendered = entry_lines_plain(&entry);
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("Round 3"));
    }

    #[test]
    fn test_chat_entry_error_note() {
        let entry = ChatEntry::ErrorNote("something failed".to_string());
        let rendered = entry_lines_plain(&entry);
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("✗"));
        assert!(rendered[0].contains("something failed"));
    }

    #[test]
    fn test_chat_entry_stage_note() {
        let entry = ChatEntry::StageNote("Planning...".to_string());
        let rendered = entry_lines_plain(&entry);
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("◆"));
        assert!(rendered[0].contains("Planning..."));
    }

    #[test]
    fn test_chat_entry_system_note() {
        let entry = ChatEntry::SystemNote("processing...".to_string());
        let rendered = entry_lines_plain(&entry);
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("◆"));
        assert!(rendered[0].contains("processing..."));
    }

    #[test]
    fn test_chat_entry_display_lines_count() {
        // Separator = 1
        assert_eq!(chat_entry_display_lines(&ChatEntry::Separator), 1);

        // User message: header + 1 line + footer = 3
        assert_eq!(
            chat_entry_display_lines(&ChatEntry::UserMessage {
                content: "hello".to_string(),
                turn_id: 1,
            }),
            3
        );

        // User message multiline: header + 3 lines + footer = 5
        assert_eq!(
            chat_entry_display_lines(&ChatEntry::UserMessage {
                content: "a\nb\nc".to_string(),
                turn_id: 1,
            }),
            5
        );

        // Collapsed tool card = 1
        assert_eq!(
            chat_entry_display_lines(&ChatEntry::ToolCard(ToolCallCard {
                name: "t".to_string(),
                status: ToolCardStatus::Running,
                duration: None,
                args_summary: Some("a".to_string()),
                output_preview: Some("o".to_string()),
                is_error: false,
                collapsed: true,
            })),
            1
        );

        // Expanded tool card with args + output = 3
        assert_eq!(
            chat_entry_display_lines(&ChatEntry::ToolCard(ToolCallCard {
                name: "t".to_string(),
                status: ToolCardStatus::Completed,
                duration: None,
                args_summary: Some("a".to_string()),
                output_preview: Some("o".to_string()),
                is_error: false,
                collapsed: false,
            })),
            3
        );

        // Collapsed reasoning = 1
        assert_eq!(
            chat_entry_display_lines(&ChatEntry::ReasoningBlock {
                round: 1,
                content: "think\nabout it".to_string(),
                collapsed: true,
            }),
            1
        );

        // Expanded reasoning: header + 2 lines = 3
        assert_eq!(
            chat_entry_display_lines(&ChatEntry::ReasoningBlock {
                round: 1,
                content: "think\nabout it".to_string(),
                collapsed: false,
            }),
            3
        );
    }

    #[test]
    fn test_total_display_lines() {
        let entries = vec![
            ChatEntry::Separator,
            ChatEntry::UserMessage {
                content: "hi".to_string(),
                turn_id: 1,
            },
            ChatEntry::SystemNote("processing".to_string()),
        ];
        // 1 + 3 + 1 = 5
        assert_eq!(total_display_lines(&entries), 5);
    }

    #[test]
    fn test_push_chat_entry_cap() {
        let mut entries = Vec::new();
        for i in 0..(TUI_MAX_CHAT_ENTRIES + 5) {
            push_chat_entry(&mut entries, ChatEntry::SystemNote(format!("note-{}", i)));
        }
        assert_eq!(entries.len(), TUI_MAX_CHAT_ENTRIES);
        // First entry should be note-5 (0..4 evicted)
        if let ChatEntry::SystemNote(text) = &entries[0] {
            assert_eq!(text, "note-5");
        } else {
            panic!("expected SystemNote");
        }
    }

    #[test]
    fn test_toggle_all_tool_cards() {
        let mut entries = vec![
            ChatEntry::ToolCard(ToolCallCard {
                name: "a".to_string(),
                status: ToolCardStatus::Completed,
                duration: None,
                args_summary: None,
                output_preview: None,
                is_error: false,
                collapsed: true,
            }),
            ChatEntry::SystemNote("note".to_string()),
            ChatEntry::ToolCard(ToolCallCard {
                name: "b".to_string(),
                status: ToolCardStatus::Running,
                duration: None,
                args_summary: None,
                output_preview: None,
                is_error: false,
                collapsed: true,
            }),
        ];
        toggle_all_tool_cards(&mut entries);
        if let ChatEntry::ToolCard(ref card) = entries[0] {
            assert!(!card.collapsed);
        }
        if let ChatEntry::ToolCard(ref card) = entries[2] {
            assert!(!card.collapsed);
        }
        // Toggle back
        toggle_all_tool_cards(&mut entries);
        if let ChatEntry::ToolCard(ref card) = entries[0] {
            assert!(card.collapsed);
        }
    }

    #[test]
    fn test_toggle_all_reasoning_blocks() {
        let mut entries = vec![
            ChatEntry::ReasoningBlock {
                round: 1,
                content: "think".to_string(),
                collapsed: true,
            },
            ChatEntry::Separator,
            ChatEntry::ReasoningBlock {
                round: 2,
                content: "more".to_string(),
                collapsed: true,
            },
        ];
        toggle_all_reasoning_blocks(&mut entries);
        if let ChatEntry::ReasoningBlock { collapsed, .. } = &entries[0] {
            assert!(!collapsed);
        }
        if let ChatEntry::ReasoningBlock { collapsed, .. } = &entries[2] {
            assert!(!collapsed);
        }
    }

    #[test]
    fn test_effective_chat_scroll_auto_follow() {
        let entries = vec![
            ChatEntry::UserMessage {
                content: "hi".to_string(),
                turn_id: 1,
            },
            ChatEntry::AssistantMessage {
                content: "hello there\nline2\nline3".to_string(),
                turn_id: 1,
            },
        ];
        // total display lines = 3 + 5 = 8
        let view = TuiSessionViewState {
            scroll_offset: 0,
            auto_follow: true,
            body_height: 5,
        };
        let scroll = effective_chat_scroll(&entries, &view);
        // auto_follow: total(8) - body_height(5) = 3
        assert_eq!(scroll, 3);
    }

    #[test]
    fn test_effective_chat_scroll_manual() {
        let entries = vec![
            ChatEntry::SystemNote("a".to_string()),
            ChatEntry::SystemNote("b".to_string()),
            ChatEntry::SystemNote("c".to_string()),
            ChatEntry::SystemNote("d".to_string()),
            ChatEntry::SystemNote("e".to_string()),
        ];
        // 5 entries × 1 line each = 5 display lines
        let view = TuiSessionViewState {
            scroll_offset: 2,
            auto_follow: false,
            body_height: 3,
        };
        let scroll = effective_chat_scroll(&entries, &view);
        // manual: min(scroll_offset=2, total(5)-body(3)=2) = 2
        assert_eq!(scroll, 2);
    }

    #[test]
    fn test_event_to_entries_tool_call_start_compact() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallStart,
            "tool_call_start: shell | args_preview: {\"command\":\"cargo build\"}",
            1,
            Some("shell"),
            Some("call-1"),
            None,
            false,
        );
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let entries = event_to_entries(&event, &mut viz);
        assert!(!entries.is_empty());
        // Should produce a ToolCard with Running status
        let tool_card = entries.iter().find(|e| matches!(e, ChatEntry::ToolCard(_)));
        assert!(tool_card.is_some(), "expected ToolCard in entries");
        if let Some(ChatEntry::ToolCard(card)) = tool_card {
            assert_eq!(card.name, "shell");
            assert!(matches!(card.status, ToolCardStatus::Running));
            assert!(card.collapsed); // default collapsed in compact mode
        }
    }

    #[test]
    fn test_event_to_entries_tool_call_end_with_result() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallEnd,
            "tool_call_end: read (ok) | result_preview: README.md contents",
            1,
            Some("read"),
            Some("call-1"),
            Some(42),
            false,
        );
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        let entries = event_to_entries(&event, &mut viz);
        let tool_card = entries.iter().find(|e| matches!(e, ChatEntry::ToolCard(_)));
        assert!(tool_card.is_some());
        if let Some(ChatEntry::ToolCard(card)) = tool_card {
            assert_eq!(card.name, "read");
            assert!(matches!(card.status, ToolCardStatus::Completed));
            assert!(!card.is_error);
            assert!(card.duration.is_some());
            assert!(card.output_preview.is_some());
        }
    }

    #[test]
    fn test_event_to_entries_tool_call_end_error() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallEnd,
            "tool_call_end: write (error) | result_preview: permission denied",
            1,
            Some("write"),
            Some("call-1"),
            Some(100),
            true,
        );
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        let entries = event_to_entries(&event, &mut viz);
        let tool_card = entries.iter().find(|e| matches!(e, ChatEntry::ToolCard(_)));
        assert!(tool_card.is_some());
        if let Some(ChatEntry::ToolCard(card)) = tool_card {
            assert_eq!(card.name, "write");
            assert!(matches!(card.status, ToolCardStatus::Failed));
            assert!(card.is_error);
        }
    }

    #[test]
    fn test_event_to_entries_reasoning_collapsed_by_default() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::Reasoning,
            "analyzing the code",
            1,
            None,
            None,
            None,
            false,
        );
        let mut viz = ReplVisualizationState::new(false);
        // show_thinking is false by default → reasoning collapsed
        let entries = event_to_entries(&event, &mut viz);
        let reasoning = entries
            .iter()
            .find(|e| matches!(e, ChatEntry::ReasoningBlock { .. }));
        assert!(reasoning.is_some());
        if let Some(ChatEntry::ReasoningBlock { collapsed, .. }) = reasoning {
            assert!(collapsed, "reasoning should be collapsed by default");
        }
    }

    #[test]
    fn test_event_to_entries_reasoning_expanded_when_show_thinking() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::Reasoning,
            "analyzing the code",
            1,
            None,
            None,
            None,
            false,
        );
        let mut viz = ReplVisualizationState::new(false);
        viz.show_thinking = true;
        let entries = event_to_entries(&event, &mut viz);
        let reasoning = entries
            .iter()
            .find(|e| matches!(e, ChatEntry::ReasoningBlock { .. }));
        assert!(reasoning.is_some());
        if let Some(ChatEntry::ReasoningBlock { collapsed, .. }) = reasoning {
            assert!(
                !collapsed,
                "reasoning should be expanded when show_thinking is on"
            );
        }
    }

    #[test]
    fn test_event_to_entries_round_separator_normal() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallStart,
            "tool_call_start: shell",
            2,
            Some("shell"),
            Some("call-1"),
            None,
            false,
        );
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        viz.last_emitted_round = 1;
        let entries = event_to_entries(&event, &mut viz);
        // Should start with RoundSeparator
        assert!(
            matches!(&entries[0], ChatEntry::RoundSeparator { round: 2 }),
            "expected RoundSeparator for round 2"
        );
    }

    #[test]
    fn test_event_to_entries_no_round_sep_compact() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::ToolCallStart,
            "tool_call_start: shell",
            2,
            Some("shell"),
            Some("call-1"),
            None,
            false,
        );
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Compact;
        viz.last_emitted_round = 1;
        let entries = event_to_entries(&event, &mut viz);
        // Compact mode: no round separator
        assert!(
            !entries
                .iter()
                .any(|e| matches!(e, ChatEntry::RoundSeparator { .. })),
            "compact mode should not emit round separators"
        );
    }

    #[test]
    fn test_event_to_entries_permission() {
        let event = mk_event(
            ndc_core::AgentExecutionEventKind::PermissionAsked,
            "tool shell requires permission",
            1,
            None,
            None,
            None,
            false,
        );
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        let entries = event_to_entries(&event, &mut viz);
        assert!(viz.permission_blocked);
        let has_note = entries
            .iter()
            .any(|e| matches!(e, ChatEntry::PermissionNote(_)));
        let has_hint = entries
            .iter()
            .any(|e| matches!(e, ChatEntry::PermissionHint(_)));
        assert!(has_note, "expected PermissionNote");
        assert!(has_hint, "expected PermissionHint");
    }

    #[test]
    fn test_event_to_entries_stage() {
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "planning phase".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Planning),
            workflow_detail: Some("analyzing requirements".to_string()),
            workflow_stage_index: Some(1),
            workflow_stage_total: Some(3),
        };
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Normal;
        let entries = event_to_entries(&event, &mut viz);
        let stage = entries
            .iter()
            .find(|e| matches!(e, ChatEntry::StageNote(_)));
        assert!(stage.is_some(), "expected StageNote");
        if let Some(ChatEntry::StageNote(text)) = stage {
            assert!(text.contains("Planning"));
        }
    }

    #[test]
    fn test_style_chat_entries_mixed() {
        let entries = vec![
            ChatEntry::Separator,
            ChatEntry::UserMessage {
                content: "build it".to_string(),
                turn_id: 1,
            },
            ChatEntry::SystemNote("processing...".to_string()),
            ChatEntry::ToolCard(ToolCallCard {
                name: "shell".to_string(),
                status: ToolCardStatus::Completed,
                duration: Some("2.0s".to_string()),
                args_summary: None,
                output_preview: None,
                is_error: false,
                collapsed: true,
            }),
            ChatEntry::AssistantMessage {
                content: "Done!".to_string(),
                turn_id: 1,
            },
        ];
        let lines = style_chat_entries(&entries);
        // Separator(1) + User(3) + SystemNote(1) + ToolCard collapsed(1) + Assistant(3) = 9
        assert_eq!(lines.len(), 9);
        // Verify user message header is present
        let plain: Vec<String> = lines.iter().map(line_plain).collect();
        assert!(plain.iter().any(|l| l.contains("You [#1]")));
        assert!(plain.iter().any(|l| l.contains("build it")));
        assert!(plain.iter().any(|l| l.contains("processing...")));
        assert!(plain.iter().any(|l| l.contains("shell")));
        assert!(plain.iter().any(|l| l.contains("Assistant [#1]")));
        assert!(plain.iter().any(|l| l.contains("Done!")));
    }

    #[test]
    fn test_diff_preview_display_lines_collapsed() {
        let entry = ChatEntry::DiffPreview {
            path: "src/main.rs".to_string(),
            lines: vec![
                DiffLine::Removed("old line".to_string()),
                DiffLine::Added("new line".to_string()),
            ],
            collapsed: true,
        };
        assert_eq!(chat_entry_display_lines(&entry), 1);
    }

    #[test]
    fn test_diff_preview_display_lines_expanded() {
        let entry = ChatEntry::DiffPreview {
            path: "src/main.rs".to_string(),
            lines: vec![
                DiffLine::Context("context".to_string()),
                DiffLine::Removed("old".to_string()),
                DiffLine::Added("new".to_string()),
            ],
            collapsed: false,
        };
        // header + 3 diff lines
        assert_eq!(chat_entry_display_lines(&entry), 4);
    }

    #[test]
    fn test_diff_preview_rendering_expanded() {
        let entries = vec![ChatEntry::DiffPreview {
            path: "lib.rs".to_string(),
            lines: vec![
                DiffLine::Context("fn main() {".to_string()),
                DiffLine::Removed("    old()".to_string()),
                DiffLine::Added("    new()".to_string()),
                DiffLine::Context("}".to_string()),
            ],
            collapsed: false,
        }];
        let styled = style_chat_entries(&entries);
        assert_eq!(styled.len(), 5); // header + 4 diff lines
        let plain: Vec<String> = styled.iter().map(line_plain).collect();
        assert!(plain[0].contains("Diff:"));
        assert!(plain[0].contains("lib.rs"));
        assert!(plain[1].contains("fn main()"));
        assert!(plain[2].contains("- "));
        assert!(plain[2].contains("old()"));
        assert!(plain[3].contains("+ "));
        assert!(plain[3].contains("new()"));
    }

    #[test]
    fn test_diff_preview_rendering_collapsed() {
        let entries = vec![ChatEntry::DiffPreview {
            path: "test.rs".to_string(),
            lines: vec![DiffLine::Added("line".to_string())],
            collapsed: true,
        }];
        let styled = style_chat_entries(&entries);
        assert_eq!(styled.len(), 1); // only header
        let plain = line_plain(&styled[0]);
        assert!(plain.contains("Diff:"));
        assert!(plain.contains("test.rs"));
    }

    #[test]
    fn test_tool_card_write_tool_accent_green() {
        let entries = vec![ChatEntry::ToolCard(ToolCallCard {
            name: "write_file".to_string(),
            status: ToolCardStatus::Completed,
            duration: None,
            args_summary: None,
            output_preview: None,
            is_error: false,
            collapsed: true,
        })];
        let styled = style_chat_entries(&entries);
        assert_eq!(styled.len(), 1);
        // The icon span should use green (success) color for write tools
        let spans = &styled[0].spans;
        assert!(!spans.is_empty());
        // First span is the icon — verify it uses green
        let theme = TuiTheme::default_dark();
        assert_eq!(spans[0].style.fg, Some(theme.success));
    }

    #[test]
    fn test_tool_card_shell_tool_accent_orange() {
        let entries = vec![ChatEntry::ToolCard(ToolCallCard {
            name: "run_command".to_string(),
            status: ToolCardStatus::Completed,
            duration: None,
            args_summary: None,
            output_preview: None,
            is_error: false,
            collapsed: true,
        })];
        let styled = style_chat_entries(&entries);
        let spans = &styled[0].spans;
        let theme = TuiTheme::default_dark();
        assert_eq!(spans[0].style.fg, Some(theme.warning));
    }

    #[test]
    fn test_tool_card_other_tool_default_accent() {
        let entries = vec![ChatEntry::ToolCard(ToolCallCard {
            name: "search_code".to_string(),
            status: ToolCardStatus::Completed,
            duration: None,
            args_summary: None,
            output_preview: None,
            is_error: false,
            collapsed: true,
        })];
        let styled = style_chat_entries(&entries);
        let spans = &styled[0].spans;
        let theme = TuiTheme::default_dark();
        assert_eq!(spans[0].style.fg, Some(theme.tool_accent));
    }

    #[test]
    fn test_drain_live_chat_entries_renders() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::ToolCallStart,
            timestamp: chrono::Utc::now(),
            message: "tool_call_start: read | args_preview: {\"path\":\".\"}".to_string(),
            round: 1,
            tool_name: Some("read".to_string()),
            tool_call_id: Some("call-1".to_string()),
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        tx.send(ndc_core::AgentSessionExecutionEvent {
            session_id: "session-a".to_string(),
            event: event.clone(),
        })
        .unwrap();

        let mut recv = Some(rx);
        let mut viz = ReplVisualizationState::new(false);
        let mut entries: Vec<ChatEntry> = Vec::new();
        let rendered =
            drain_live_chat_entries(&mut recv, Some("session-a"), &mut viz, &mut entries);
        assert!(rendered);
        assert!(!entries.is_empty());
        // Should contain a ToolCard entry
        assert!(entries.iter().any(|e| matches!(e, ChatEntry::ToolCard(_))));
    }

    #[test]
    fn test_drain_live_chat_entries_ignores_other_sessions() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        tx.send(ndc_core::AgentSessionExecutionEvent {
            session_id: "session-b".to_string(),
            event: ndc_core::AgentExecutionEvent {
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
            },
        })
        .unwrap();

        let mut recv = Some(rx);
        let mut viz = ReplVisualizationState::new(false);
        let mut entries: Vec<ChatEntry> = Vec::new();
        let rendered =
            drain_live_chat_entries(&mut recv, Some("session-a"), &mut viz, &mut entries);
        assert!(!rendered);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_event_to_entries_verbose_full_round() {
        let events = vec![
            ndc_core::AgentExecutionEvent {
                kind: ndc_core::AgentExecutionEventKind::WorkflowStage,
                timestamp: chrono::Utc::now(),
                message: "planning".to_string(),
                round: 1,
                tool_name: None,
                tool_call_id: None,
                duration_ms: None,
                is_error: false,
                workflow_stage: Some(ndc_core::AgentWorkflowStage::Planning),
                workflow_detail: Some("".to_string()),
                workflow_stage_index: Some(1),
                workflow_stage_total: Some(3),
            },
            mk_event(
                ndc_core::AgentExecutionEventKind::Reasoning,
                "analyze the code",
                1,
                None,
                None,
                None,
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallStart,
                "tool_call_start: read | args_preview: {\"path\":\"src/\"}",
                1,
                Some("read"),
                Some("call-1"),
                None,
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallEnd,
                "tool_call_end: read (ok) | args_preview: {\"path\":\"src/\"} | result_preview: main.rs lib.rs",
                1,
                Some("read"),
                Some("call-1"),
                Some(15),
                false,
            ),
        ];
        let mut viz = ReplVisualizationState::new(false);
        viz.verbosity = DisplayVerbosity::Verbose;
        viz.show_thinking = true;
        viz.expand_tool_cards = true;

        let entries = render_entries_snapshot(&events, &mut viz);

        // Should have stage, reasoning, tool start card, tool end card
        let has_stage = entries.iter().any(|e| matches!(e, ChatEntry::StageNote(_)));
        let has_reasoning = entries.iter().any(|e| {
            matches!(
                e,
                ChatEntry::ReasoningBlock {
                    collapsed: false,
                    ..
                }
            )
        });
        let tool_cards: Vec<_> = entries
            .iter()
            .filter(|e| matches!(e, ChatEntry::ToolCard(_)))
            .collect();
        assert!(has_stage, "expected stage entry");
        assert!(has_reasoning, "expected expanded reasoning");
        assert_eq!(tool_cards.len(), 2, "expected 2 tool cards (start + end)");
    }
}
