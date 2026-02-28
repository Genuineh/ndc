//! Layout Manager — TUI layout calculation, display utilities, and formatting.
//!
//! Extracted from `repl.rs` (SEC-S1 God Object refactoring).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::{
    ReplCommandCompletionState, ReplVisualizationState, TuiTheme, canonical_slash_command,
    matching_slash_commands, parse_slash_tokens, slash_argument_options,
};

pub const TUI_SCROLL_STEP: usize = 3;
pub const TIMELINE_CACHE_MAX_EVENTS: usize = 1_000;
pub const WORKFLOW_STAGE_ORDER: &[&str] = &[
    "planning",
    "discovery",
    "executing",
    "verifying",
    "completing",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayVerbosity {
    Compact,
    Normal,
    Verbose,
}

impl DisplayVerbosity {
    pub fn next(self) -> Self {
        match self {
            Self::Compact => Self::Normal,
            Self::Normal => Self::Verbose,
            Self::Verbose => Self::Compact,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Normal => "normal",
            Self::Verbose => "verbose",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "compact" | "c" => Some(Self::Compact),
            "normal" | "n" => Some(Self::Normal),
            "verbose" | "v" | "debug" => Some(Self::Verbose),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowOverviewMode {
    Compact,
    Verbose,
}

impl WorkflowOverviewMode {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        let Some(raw) = value else {
            return Ok(Self::Verbose);
        };
        match raw.to_ascii_lowercase().as_str() {
            "compact" => Ok(Self::Compact),
            "verbose" => Ok(Self::Verbose),
            _ => Err("Usage: /workflow [compact|verbose]".to_string()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            WorkflowOverviewMode::Compact => "compact",
            WorkflowOverviewMode::Verbose => "verbose",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TuiSessionViewState {
    pub scroll_offset: usize,
    pub auto_follow: bool,
    pub body_height: usize,
}

impl Default for TuiSessionViewState {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            auto_follow: true,
            body_height: 1,
        }
    }
}

pub fn short_session_id(value: Option<&str>) -> String {
    let session = value.unwrap_or("-");
    let max = 12usize;
    if session.chars().count() <= max {
        return session.to_string();
    }
    let prefix = session.chars().take(max).collect::<String>();
    format!("{}…", prefix)
}

pub fn workflow_progress_descriptor(
    stage_name: Option<&str>,
    stage_index: Option<u32>,
    stage_total: Option<u32>,
) -> String {
    let (index, total) = if let (Some(index), Some(total)) = (stage_index, stage_total) {
        if index > 0 && total > 0 {
            (index, total)
        } else {
            let Some(stage) = stage_name.and_then(ndc_core::AgentWorkflowStage::parse) else {
                return "-".to_string();
            };
            (stage.index(), ndc_core::AgentWorkflowStage::TOTAL_STAGES)
        }
    } else {
        let Some(stage) = stage_name.and_then(ndc_core::AgentWorkflowStage::parse) else {
            return "-".to_string();
        };
        (stage.index(), ndc_core::AgentWorkflowStage::TOTAL_STAGES)
    };
    let percent = if total == 0 { 0 } else { (index * 100) / total };
    format!("{}%({}/{})", percent, index, total)
}

#[cfg(test)]
pub fn build_status_line(
    status: &crate::AgentStatus,
    viz_state: &ReplVisualizationState,
    is_processing: bool,
    session_view: &TuiSessionViewState,
    stream_state: &str,
) -> String {
    let workflow_stage = viz_state.current_workflow_stage.as_deref().unwrap_or("-");
    let workflow_progress = workflow_progress_descriptor(
        viz_state.current_workflow_stage.as_deref(),
        viz_state.current_workflow_stage_index,
        viz_state.current_workflow_stage_total,
    );
    let workflow_elapsed_ms = viz_state
        .current_workflow_stage_started_at
        .map(|started| {
            chrono::Utc::now()
                .signed_duration_since(started)
                .num_milliseconds()
                .max(0) as u64
        })
        .unwrap_or(0);
    let usage = if viz_state.show_usage_metrics {
        format!(
            "tok_round={} tok_session={}",
            viz_state.latest_round_token_total, viz_state.session_token_total
        )
    } else {
        "tok=hidden".to_string()
    };
    format!(
        "provider={} model={} session={} workflow={} workflow_progress={} workflow_ms={} blocked={} {} thinking={} details={} cards={} stream={} hidden_thinking={} scroll={} state={}",
        status.provider,
        status.model,
        short_session_id(status.session_id.as_deref()),
        workflow_stage,
        workflow_progress,
        workflow_elapsed_ms,
        if viz_state.permission_blocked {
            "yes"
        } else {
            "no"
        },
        usage,
        if viz_state.show_thinking { "on" } else { "off" },
        if viz_state.show_tool_details {
            "on"
        } else {
            "off"
        },
        if viz_state.expand_tool_cards {
            "expanded"
        } else {
            "collapsed"
        },
        stream_state,
        viz_state.hidden_thinking_round_hints.len(),
        if session_view.auto_follow {
            "follow"
        } else {
            "manual"
        },
        if is_processing { "processing" } else { "idle" }
    )
}

pub fn tool_status_narrative(tool_name: Option<&str>) -> &'static str {
    match tool_name {
        Some("read" | "read_file") => "Reading file...",
        Some("grep" | "glob" | "list" | "list_dir") => "Searching codebase...",
        Some("write" | "write_file" | "edit" | "edit_file") => "Making edits...",
        Some("shell" | "bash") => "Running command...",
        Some("ndc_task_create" | "ndc_task_list" | "ndc_task_update" | "ndc_task_verify") => {
            "Managing tasks..."
        }
        Some("webfetch" | "websearch") => "Searching web...",
        _ => "Working...",
    }
}

pub fn build_title_bar<'a>(
    status: &crate::AgentStatus,
    is_processing: bool,
    active_tool: Option<&str>,
    theme: &TuiTheme,
) -> Line<'a> {
    let project_name = status
        .project_root
        .as_deref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("-");

    let state_text = if is_processing {
        tool_status_narrative(active_tool)
    } else {
        "idle"
    };
    let state_color = if is_processing {
        theme.warning
    } else {
        theme.text_muted
    };

    Line::from(vec![
        Span::styled(
            " NDC ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default()),
        Span::styled(project_name.to_string(), Style::default().fg(theme.primary)),
        Span::styled(
            format!("  {}  ", short_session_id(status.session_id.as_deref())),
            Style::default().fg(theme.text_dim),
        ),
        Span::styled(
            format!("{}  ", status.model),
            Style::default().fg(theme.success),
        ),
        Span::styled(state_text.to_string(), Style::default().fg(state_color)),
    ])
}

pub fn build_workflow_progress_bar<'a>(
    viz_state: &ReplVisualizationState,
    theme: &TuiTheme,
) -> Line<'a> {
    let current = viz_state.current_workflow_stage.as_deref();
    let current_idx = current.and_then(|s| WORKFLOW_STAGE_ORDER.iter().position(|w| *w == s));

    let mut spans: Vec<Span<'a>> = vec![Span::raw(" ")];
    for (i, stage) in WORKFLOW_STAGE_ORDER.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ── ", Style::default().fg(theme.border_dim)));
        }
        let (text, style) = match current_idx {
            Some(ci) if i < ci => (stage.to_string(), Style::default().fg(theme.progress_done)),
            Some(ci) if i == ci => (
                format!("[{}]", stage),
                Style::default()
                    .fg(theme.progress_active)
                    .add_modifier(Modifier::BOLD),
            ),
            _ => (
                stage.to_string(),
                Style::default().fg(theme.progress_pending),
            ),
        };
        spans.push(Span::styled(text, style));
    }

    // Append Scene badge after progress bar
    let scene = super::scene::classify_scene(current, None);
    spans.push(Span::styled("  ", Style::default()));
    spans.push(Span::styled(
        format!("[{}]", scene.badge_label()),
        Style::default()
            .fg(scene.accent_color())
            .add_modifier(Modifier::BOLD),
    ));

    Line::from(spans)
}

pub fn build_permission_bar<'a>(
    viz_state: &ReplVisualizationState,
    theme: &TuiTheme,
) -> Vec<Line<'a>> {
    let msg = match &viz_state.permission_pending_message {
        Some(m) => m.clone(),
        None => "Awaiting confirmation...".to_string(),
    };
    vec![
        Line::from(vec![
            Span::styled(
                " ⚠ Permission Required ",
                Style::default()
                    .fg(Color::Black)
                    .bg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", msg), Style::default().fg(theme.warning)),
        ]),
        Line::from(vec![Span::styled(
            "   [y] Allow  [n] Deny  [a] Session allow  [p] Permanent allow",
            Style::default().fg(theme.text_muted),
        )]),
    ]
}

pub fn build_status_hint_bar<'a>(
    input: &str,
    completion: Option<&ReplCommandCompletionState>,
    viz_state: &ReplVisualizationState,
    stream_state: &str,
    theme: &TuiTheme,
    is_processing: bool,
) -> Line<'a> {
    if let Some((_raw, norm, args, trailing_space)) = parse_slash_tokens(input)
        && completion.is_none()
    {
        if args.is_empty() && !trailing_space {
            let matches = matching_slash_commands(&norm);
            let cmds: String = matches
                .iter()
                .map(|s| s.command)
                .collect::<Vec<_>>()
                .join("  ");
            return Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(cmds, Style::default().fg(theme.text_muted)),
                Span::styled("  Tab: complete", Style::default().fg(theme.text_dim)),
            ]);
        }
        if let Some(opts) = slash_argument_options(&norm) {
            let opts_str = opts.join("  ");
            return Line::from(vec![
                Span::styled(
                    format!(" {}: ", canonical_slash_command(&norm)),
                    Style::default().fg(theme.primary),
                ),
                Span::styled(opts_str, Style::default().fg(theme.text_muted)),
            ]);
        }
    }

    if let Some(comp) = completion {
        let label = format!(
            " [{}/{}] {} ",
            comp.selected_index + 1,
            comp.suggestions.len(),
            comp.suggestions
                .get(comp.selected_index)
                .map(String::as_str)
                .unwrap_or(""),
        );
        return Line::from(vec![
            Span::styled(label, Style::default().fg(theme.primary)),
            Span::styled(
                "  Tab/Shift+Tab: cycle",
                Style::default().fg(theme.text_dim),
            ),
        ]);
    }

    let sep = Span::styled(" │ ", Style::default().fg(theme.border_dim));
    let mut spans = vec![
        Span::styled(" /help", Style::default().fg(theme.text_muted)),
        sep.clone(),
    ];

    if viz_state.show_usage_metrics && viz_state.session_token_total > 0 {
        let round = viz_state.latest_round_token_total;
        let session = viz_state.session_token_total;
        let bar = token_progress_bar(session, 128_000);
        spans.push(Span::styled(
            format!(
                "tok {}/{} {}",
                format_token_count(round),
                format_token_count(session),
                bar
            ),
            Style::default().fg(theme.text_dim),
        ));
        spans.push(sep.clone());
    }

    spans.push(Span::styled(
        format!("stream:{}", stream_state),
        Style::default().fg(theme.text_dim),
    ));
    spans.push(sep.clone());
    if is_processing {
        spans.push(Span::styled(
            "Ctrl+C interrupt  PgUp/Dn scroll  Esc exit",
            Style::default().fg(theme.text_dim),
        ));
    } else {
        spans.push(Span::styled(
            "Shift+Enter newline  ↑↓ history  PgUp/Dn scroll  Ctrl+C/Esc exit",
            Style::default().fg(theme.text_dim),
        ));
    }

    Line::from(spans)
}

pub fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn token_progress_bar(current: u64, capacity: u64) -> String {
    let ratio = if capacity == 0 {
        0.0
    } else {
        (current as f64 / capacity as f64).clamp(0.0, 1.0)
    };
    let filled = (ratio * 8.0).round() as usize;
    let empty = 8 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

pub fn truncate_output(text: &str, max_chars: usize) -> (String, bool) {
    if text.len() <= max_chars {
        (text.to_string(), false)
    } else {
        let mut end = max_chars;
        while !text.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        (text[..end].to_string(), true)
    }
}

pub fn capitalize_stage(stage: &str) -> String {
    let mut chars = stage.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

pub fn extract_tool_summary(tool_name: &str, args_json: &str) -> String {
    let extract_field = |field: &str| -> Option<String> {
        let key = format!("\"{}\":", field);
        let idx = args_json.find(&key)?;
        let rest = &args_json[idx + key.len()..];
        let rest = rest.trim_start();
        if let Some(inner) = rest.strip_prefix('"') {
            let end = inner.find('"')?;
            Some(inner[..end].to_string())
        } else {
            let end = rest
                .find(&[',', '}', '\n'] as &[char])
                .unwrap_or(rest.len());
            Some(rest[..end].trim().to_string())
        }
    };

    match tool_name {
        "shell" | "bash" => extract_field("command").unwrap_or_default(),
        "read" | "read_file" => extract_field("path")
            .or_else(|| extract_field("file_path"))
            .unwrap_or_default(),
        "write" | "write_file" | "edit" | "edit_file" => extract_field("path")
            .or_else(|| extract_field("file_path"))
            .unwrap_or_default(),
        "grep" | "glob" => extract_field("pattern")
            .or_else(|| extract_field("query"))
            .unwrap_or_default(),
        "list" | "list_dir" => extract_field("path").unwrap_or_else(|| ".".to_string()),
        "webfetch" | "websearch" => extract_field("url")
            .or_else(|| extract_field("query"))
            .unwrap_or_default(),
        _ => {
            if let Some(val) = extract_field("path")
                .or_else(|| extract_field("command"))
                .or_else(|| extract_field("query"))
            {
                val
            } else {
                let clean = args_json
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .trim();
                let (t, _) = truncate_output(clean, 60);
                t
            }
        }
    }
}

pub fn format_duration_ms(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{}ms", ms)
    }
}

pub fn input_line_count(input: &str) -> u16 {
    let lines = input.chars().filter(|c| *c == '\n').count() as u16 + 1;
    lines.clamp(1, 4)
}

pub fn tui_layout_constraints(has_permission: bool, input_lines: u16) -> Vec<Constraint> {
    let input_height = input_lines + 2;
    let mut c = vec![
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
    ];
    if has_permission {
        c.push(Constraint::Length(2));
    }
    c.push(Constraint::Length(1));
    c.push(Constraint::Length(input_height));
    c
}

/// Split a conversation body area horizontally into (conversation, todo_sidebar).
///
/// When `show_todo` is true and the area is wide enough (>= 60 columns),
/// the right 28 columns are allocated to the TODO sidebar.
/// Otherwise the full area is returned for conversation.
pub fn tui_session_split(area: Rect, show_todo: bool) -> (Rect, Option<Rect>) {
    if !show_todo || area.width < 60 {
        return (area, None);
    }
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(30), Constraint::Length(28)])
        .split(area);
    (chunks[0], Some(chunks[1]))
}

pub fn resolve_stream_state(
    is_processing: bool,
    live_events_enabled: bool,
    has_live_receiver: bool,
) -> &'static str {
    if !live_events_enabled {
        return "off";
    }
    if !is_processing {
        return "ready";
    }
    if has_live_receiver { "live" } else { "poll" }
}

pub fn apply_stream_command(
    viz_state: &mut ReplVisualizationState,
    arg: Option<&str>,
) -> Result<String, String> {
    match arg.map(|value| value.to_ascii_lowercase()) {
        None => {
            viz_state.live_events_enabled = !viz_state.live_events_enabled;
        }
        Some(mode) if matches!(mode.as_str(), "on" | "enable" | "enabled" | "true" | "1") => {
            viz_state.live_events_enabled = true;
        }
        Some(mode)
            if matches!(
                mode.as_str(),
                "off" | "disable" | "disabled" | "false" | "0"
            ) =>
        {
            viz_state.live_events_enabled = false;
        }
        Some(mode) if matches!(mode.as_str(), "status" | "show") => {}
        Some(_) => {
            return Err("Usage: /stream [on|off|status]".to_string());
        }
    }
    Ok(format!(
        "Realtime stream: {}",
        if viz_state.live_events_enabled {
            "ON"
        } else {
            "OFF (polling fallback only)"
        }
    ))
}

pub fn apply_tokens_command(
    viz_state: &mut ReplVisualizationState,
    arg: Option<&str>,
) -> Result<String, String> {
    let mode = arg
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "status".to_string());
    match mode.as_str() {
        "status" | "show" => {
            viz_state.show_usage_metrics = true;
        }
        "hide" | "off" => {
            viz_state.show_usage_metrics = false;
        }
        "reset" => {
            viz_state.session_token_total = 0;
            viz_state.latest_round_token_total = 0;
        }
        _ => {
            return Err("Usage: /tokens [show|hide|reset|status]".to_string());
        }
    }
    Ok(format!(
        "Token metrics: display={} round_total={} session_total={}",
        if viz_state.show_usage_metrics {
            "ON"
        } else {
            "OFF"
        },
        viz_state.latest_round_token_total,
        viz_state.session_token_total
    ))
}

#[cfg(test)]
pub fn calc_log_scroll(log_count: usize, body_height: usize) -> u16 {
    log_count.saturating_sub(body_height).min(u16::MAX as usize) as u16
}

pub fn calc_log_scroll_usize(log_count: usize, body_height: usize) -> usize {
    log_count.saturating_sub(body_height)
}

pub fn effective_log_scroll(log_count: usize, session_view: &TuiSessionViewState) -> usize {
    let max_scroll = calc_log_scroll_usize(log_count, session_view.body_height);
    if session_view.auto_follow {
        max_scroll
    } else {
        session_view.scroll_offset.min(max_scroll)
    }
}

pub fn append_timeline_events(
    timeline: &mut Vec<ndc_core::AgentExecutionEvent>,
    incoming: &[ndc_core::AgentExecutionEvent],
    capacity: usize,
) {
    timeline.extend_from_slice(incoming);
    if timeline.len() > capacity {
        let overflow = timeline.len() - capacity;
        timeline.drain(0..overflow);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReplRuntimeMetrics {
    pub tool_calls_total: usize,
    pub tool_calls_failed: usize,
    pub tool_duration_samples: usize,
    pub tool_duration_total_ms: u64,
    pub permission_waits: usize,
    pub error_events: usize,
}

impl ReplRuntimeMetrics {
    pub fn avg_tool_duration_ms(self) -> Option<u64> {
        if self.tool_duration_samples == 0 {
            None
        } else {
            Some(self.tool_duration_total_ms / self.tool_duration_samples as u64)
        }
    }

    pub fn tool_error_rate_percent(self) -> u64 {
        if self.tool_calls_total == 0 {
            0
        } else {
            ((self.tool_calls_failed as u64) * 100) / (self.tool_calls_total as u64)
        }
    }
}

pub fn compute_runtime_metrics(timeline: &[ndc_core::AgentExecutionEvent]) -> ReplRuntimeMetrics {
    let mut metrics = ReplRuntimeMetrics::default();
    for event in timeline {
        match event.kind {
            ndc_core::AgentExecutionEventKind::ToolCallEnd => {
                metrics.tool_calls_total += 1;
                if event.is_error {
                    metrics.tool_calls_failed += 1;
                }
                if let Some(ms) = event.duration_ms {
                    metrics.tool_duration_samples += 1;
                    metrics.tool_duration_total_ms += ms;
                }
            }
            ndc_core::AgentExecutionEventKind::PermissionAsked => {
                metrics.permission_waits += 1;
            }
            ndc_core::AgentExecutionEventKind::Error => {
                metrics.error_events += 1;
            }
            _ => {}
        }
    }
    metrics
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowStageProgress {
    pub count: usize,
    pub total_ms: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkflowProgressSummary {
    pub stages: std::collections::BTreeMap<String, WorkflowStageProgress>,
    pub current_stage: Option<String>,
    pub current_stage_active_ms: u64,
    pub history_may_be_partial: bool,
}

pub fn compute_workflow_progress_summary(
    timeline: &[ndc_core::AgentExecutionEvent],
    now: chrono::DateTime<chrono::Utc>,
) -> WorkflowProgressSummary {
    let mut summary = WorkflowProgressSummary {
        history_may_be_partial: timeline.len() >= TIMELINE_CACHE_MAX_EVENTS,
        ..WorkflowProgressSummary::default()
    };
    let mut stage_points = Vec::<(String, chrono::DateTime<chrono::Utc>)>::new();
    for event in timeline {
        let Some(info) = event.workflow_stage_info() else {
            continue;
        };
        summary
            .stages
            .entry(info.stage.to_string())
            .and_modify(|entry| entry.count += 1)
            .or_insert(WorkflowStageProgress {
                count: 1,
                total_ms: 0,
            });
        stage_points.push((info.stage.to_string(), event.timestamp));
    }

    for window in stage_points.windows(2) {
        let (stage, start) = (&window[0].0, window[0].1);
        let end = window[1].1;
        let elapsed = end.signed_duration_since(start).num_milliseconds().max(0) as u64;
        if let Some(entry) = summary.stages.get_mut(stage) {
            entry.total_ms = entry.total_ms.saturating_add(elapsed);
        }
    }

    if let Some((stage, started_at)) = stage_points.last() {
        summary.current_stage = Some(stage.clone());
        summary.current_stage_active_ms = now
            .signed_duration_since(*started_at)
            .num_milliseconds()
            .max(0) as u64;
        if let Some(entry) = summary.stages.get_mut(stage) {
            entry.total_ms = entry
                .total_ms
                .saturating_add(summary.current_stage_active_ms);
        }
    }
    summary
}

pub fn group_timeline_by_stage<'a>(
    timeline: &'a [ndc_core::AgentExecutionEvent],
) -> Vec<(String, Vec<&'a ndc_core::AgentExecutionEvent>)> {
    let mut groups = Vec::<(String, Vec<&'a ndc_core::AgentExecutionEvent>)>::new();
    let mut current_stage = "unknown".to_string();
    let mut current_events = Vec::<&'a ndc_core::AgentExecutionEvent>::new();

    for event in timeline {
        if let Some(info) = event.workflow_stage_info() {
            if !current_events.is_empty() {
                groups.push((current_stage, std::mem::take(&mut current_events)));
            }
            current_stage = info.stage.to_string();
        }
        current_events.push(event);
    }
    if !current_events.is_empty() {
        groups.push((current_stage, current_events));
    }
    groups
}

pub fn extract_preview<'a>(message: &'a str, marker: &str) -> Option<&'a str> {
    let idx = message.find(marker)?;
    let start = idx + marker.len();
    let rest = message[start..].trim_start();
    let cut = rest.find('|').unwrap_or(rest.len());
    Some(rest[..cut].trim())
}

pub fn extract_tool_args_preview(message: &str) -> Option<&str> {
    extract_preview(message, "args_preview:")
}

pub fn extract_tool_result_preview(message: &str) -> Option<&str> {
    extract_preview(message, "result_preview:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;
    use ratatui::layout::Constraint;

    #[test]
    fn test_append_timeline_events_respects_capacity() {
        let mut timeline = Vec::new();
        let mk = |idx: usize| ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::StepStart,
            timestamp: chrono::Utc::now(),
            message: format!("event-{}", idx),
            round: idx,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let incoming = vec![mk(1), mk(2), mk(3)];
        append_timeline_events(&mut timeline, &incoming, 2);
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].message, "event-2");
        assert_eq!(timeline[1].message, "event-3");
    }

    #[test]
    fn test_resolve_stream_state_variants() {
        assert_eq!(resolve_stream_state(false, false, false), "off");
        assert_eq!(resolve_stream_state(false, true, false), "ready");
        assert_eq!(resolve_stream_state(true, true, true), "live");
        assert_eq!(resolve_stream_state(true, true, false), "poll");
    }

    #[test]
    fn test_apply_stream_command() {
        let mut viz = ReplVisualizationState::new(false);
        let message = apply_stream_command(&mut viz, Some("off")).expect("off");
        assert!(!viz.live_events_enabled);
        assert!(message.contains("OFF"));

        let message = apply_stream_command(&mut viz, Some("on")).expect("on");
        assert!(viz.live_events_enabled);
        assert!(message.contains("ON"));

        let message = apply_stream_command(&mut viz, Some("status")).expect("status");
        assert!(message.contains("ON"));

        apply_stream_command(&mut viz, None).expect("toggle");
        assert!(!viz.live_events_enabled);

        let err = apply_stream_command(&mut viz, Some("bad")).expect_err("invalid mode");
        assert!(err.contains("Usage: /stream"));
    }

    #[test]
    fn test_extract_tool_result_preview() {
        let msg = "tool_call_end: read (ok) | result_preview: README.md Cargo.toml";
        assert_eq!(
            extract_tool_result_preview(msg),
            Some("README.md Cargo.toml")
        );
        assert_eq!(
            extract_tool_result_preview("tool_call_end: read (ok)"),
            None
        );
    }

    #[test]
    fn test_extract_tool_previews_combined() {
        let msg = "tool_call_end: read (ok) | args_preview: {\"path\":\"README.md\"} | result_preview: ok";
        assert_eq!(
            extract_tool_args_preview(msg),
            Some("{\"path\":\"README.md\"}")
        );
        assert_eq!(extract_tool_result_preview(msg), Some("ok"));
    }

    #[test]
    fn test_compute_workflow_progress_summary_counts_and_durations() {
        let base = chrono::Utc::now();
        let timeline = vec![
            mk_event_at(
                ndc_core::AgentExecutionEventKind::WorkflowStage,
                "workflow_stage: planning | build_prompt_and_context",
                0,
                base,
            ),
            mk_event_at(
                ndc_core::AgentExecutionEventKind::WorkflowStage,
                "workflow_stage: executing | llm_round_start",
                1,
                base + chrono::Duration::milliseconds(120),
            ),
            mk_event_at(
                ndc_core::AgentExecutionEventKind::WorkflowStage,
                "workflow_stage: completing | finalize_response_and_idle",
                1,
                base + chrono::Duration::milliseconds(360),
            ),
        ];
        let summary = compute_workflow_progress_summary(
            &timeline,
            base + chrono::Duration::milliseconds(600),
        );

        assert_eq!(summary.current_stage.as_deref(), Some("completing"));
        assert_eq!(summary.current_stage_active_ms, 240);

        let planning = summary.stages.get("planning").copied().unwrap_or_default();
        let executing = summary.stages.get("executing").copied().unwrap_or_default();
        let completing = summary
            .stages
            .get("completing")
            .copied()
            .unwrap_or_default();
        assert_eq!(planning.count, 1);
        assert_eq!(planning.total_ms, 120);
        assert_eq!(executing.count, 1);
        assert_eq!(executing.total_ms, 240);
        assert_eq!(completing.count, 1);
        assert_eq!(completing.total_ms, 240);
    }

    #[test]
    fn test_group_timeline_by_stage_contiguous_partitions() {
        let timeline = vec![
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
                ndc_core::AgentExecutionEventKind::StepStart,
                "llm_round_1_start",
                1,
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
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallEnd,
                "tool_call_end: list (ok)",
                1,
                Some("list"),
                Some("call-1"),
                Some(3),
                false,
            ),
        ];
        let grouped = group_timeline_by_stage(&timeline);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].0, "planning");
        assert_eq!(grouped[0].1.len(), 2);
        assert_eq!(grouped[1].0, "executing");
        assert_eq!(grouped[1].1.len(), 2);
    }

    #[test]
    fn test_compute_runtime_metrics_counts_errors_and_duration() {
        let timeline = vec![
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallEnd,
                "tool_call_end: list (ok)",
                1,
                Some("list"),
                Some("call-1"),
                Some(3),
                false,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::ToolCallEnd,
                "tool_call_end: write (error)",
                1,
                Some("write"),
                Some("call-2"),
                Some(7),
                true,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::PermissionAsked,
                "permission_asked: write requires approval",
                1,
                Some("write"),
                Some("call-2"),
                None,
                true,
            ),
            mk_event(
                ndc_core::AgentExecutionEventKind::Error,
                "max_tool_calls_exceeded",
                2,
                None,
                None,
                None,
                true,
            ),
        ];
        let metrics = compute_runtime_metrics(&timeline);
        assert_eq!(metrics.tool_calls_total, 2);
        assert_eq!(metrics.tool_calls_failed, 1);
        assert_eq!(metrics.permission_waits, 1);
        assert_eq!(metrics.error_events, 1);
        assert_eq!(metrics.avg_tool_duration_ms(), Some(5));
        assert_eq!(metrics.tool_error_rate_percent(), 50);
    }

    #[test]
    fn test_tui_layout_constraints_fixed_input_panel() {
        let constraints = tui_layout_constraints(false, 1);
        assert_eq!(
            constraints,
            vec![
                Constraint::Length(1), // title bar
                Constraint::Length(1), // workflow progress
                Constraint::Min(5),    // conversation
                Constraint::Length(1), // status hint
                Constraint::Length(3), // input (1 line + 2 border)
            ]
        );
    }

    #[test]
    fn test_tui_layout_constraints_with_permission() {
        let constraints = tui_layout_constraints(true, 1);
        assert_eq!(
            constraints,
            vec![
                Constraint::Length(1), // title bar
                Constraint::Length(1), // workflow progress
                Constraint::Min(5),    // conversation
                Constraint::Length(2), // permission bar
                Constraint::Length(1), // status hint
                Constraint::Length(3), // input (1 line + 2 border)
            ]
        );
    }

    #[test]
    fn test_calc_log_scroll() {
        assert_eq!(calc_log_scroll(3, 10), 0);
        assert_eq!(calc_log_scroll(25, 10), 15);
    }

    #[test]
    fn test_effective_log_scroll_auto_follow_and_manual() {
        let mut view = TuiSessionViewState {
            scroll_offset: 0,
            auto_follow: true,
            body_height: 10,
        };
        assert_eq!(effective_log_scroll(30, &view), 20);
        view.auto_follow = false;
        view.scroll_offset = 6;
        assert_eq!(effective_log_scroll(30, &view), 6);
    }

    #[test]
    fn test_workflow_overview_mode_parse() {
        assert_eq!(
            WorkflowOverviewMode::parse(None).expect("default"),
            WorkflowOverviewMode::Verbose
        );
        assert_eq!(
            WorkflowOverviewMode::parse(Some("compact")).expect("compact"),
            WorkflowOverviewMode::Compact
        );
        assert_eq!(
            WorkflowOverviewMode::parse(Some("verbose")).expect("verbose"),
            WorkflowOverviewMode::Verbose
        );
        assert!(WorkflowOverviewMode::parse(Some("unknown")).is_err());
    }

    #[test]
    fn test_short_session_id() {
        assert_eq!(short_session_id(None), "-");
        assert_eq!(short_session_id(Some("abc")), "abc");
        assert_eq!(short_session_id(Some("1234567890abcdef")), "1234567890ab…");
    }

    #[test]
    fn test_build_status_line_contains_session() {
        let status = crate::AgentStatus {
            enabled: true,
            agent_name: "build".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            session_id: Some("agent-1234567890abcdef".to_string()),
            project_id: None,
            project_root: None,
            worktree: None,
        };
        let viz = ReplVisualizationState::new(false);
        let view = TuiSessionViewState::default();
        let line = build_status_line(&status, &viz, true, &view, "live");
        assert!(line.contains("provider=openai"));
        assert!(line.contains("model=gpt-4o"));
        assert!(line.contains("session=agent-123456"));
        assert!(line.contains("workflow_progress=-"));
        assert!(line.contains("workflow_ms="));
        assert!(line.contains("blocked=no"));
        assert!(line.contains("stream=live"));
        assert!(line.contains("scroll=follow"));
        assert!(line.contains("state=processing"));
    }

    #[test]
    fn test_build_status_line_manual_scroll() {
        let status = crate::AgentStatus {
            enabled: true,
            agent_name: "build".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            session_id: Some("agent-1".to_string()),
            project_id: None,
            project_root: None,
            worktree: None,
        };
        let viz = ReplVisualizationState::new(false);
        let view = TuiSessionViewState {
            scroll_offset: 5,
            auto_follow: false,
            body_height: 10,
        };
        let line = build_status_line(&status, &viz, false, &view, "ready");
        assert!(line.contains("workflow_progress=-"));
        assert!(line.contains("workflow_ms="));
        assert!(line.contains("blocked=no"));
        assert!(line.contains("stream=ready"));
        assert!(line.contains("scroll=manual"));
        assert!(line.contains("state=idle"));
    }

    #[test]
    fn test_workflow_progress_descriptor_known_and_unknown() {
        assert_eq!(workflow_progress_descriptor(None, None, None), "-");
        assert_eq!(
            workflow_progress_descriptor(Some("unknown"), None, None),
            "-"
        );
        assert_eq!(
            workflow_progress_descriptor(Some("planning"), None, None),
            "20%(1/5)"
        );
        assert_eq!(
            workflow_progress_descriptor(Some("verifying"), None, None),
            "80%(4/5)"
        );
        assert_eq!(
            workflow_progress_descriptor(Some("executing"), Some(3), Some(5)),
            "60%(3/5)"
        );
    }

    #[test]
    fn test_workflow_progress_bar_scene_badge() {
        let theme = TuiTheme::default_dark();
        let mut viz = ReplVisualizationState::new(false);
        // No workflow stage → Chat badge
        let line = build_workflow_progress_bar(&viz, &theme);
        let plain = line_plain(&line);
        assert!(
            plain.contains("[对话]"),
            "expected Chat badge, got: {}",
            plain
        );

        // Planning → Plan badge
        viz.current_workflow_stage = Some("planning".to_string());
        let line = build_workflow_progress_bar(&viz, &theme);
        let plain = line_plain(&line);
        assert!(
            plain.contains("[规划]"),
            "expected Plan badge, got: {}",
            plain
        );

        // Executing → Implement badge
        viz.current_workflow_stage = Some("executing".to_string());
        let line = build_workflow_progress_bar(&viz, &theme);
        let plain = line_plain(&line);
        assert!(
            plain.contains("[实现]"),
            "expected Implement badge, got: {}",
            plain
        );
    }

    #[test]
    fn test_input_line_count_empty() {
        assert_eq!(input_line_count(""), 1);
    }

    #[test]
    fn test_input_line_count_single_line() {
        assert_eq!(input_line_count("hello world"), 1);
    }

    #[test]
    fn test_input_line_count_multiline() {
        assert_eq!(input_line_count("line1\nline2\nline3"), 3);
    }

    #[test]
    fn test_input_line_count_clamps_at_four() {
        assert_eq!(input_line_count("1\n2\n3\n4\n5\n6"), 4);
    }

    #[test]
    fn test_input_line_count_trailing_newline() {
        assert_eq!(input_line_count("a\n"), 2);
    }

    #[test]
    fn test_tui_layout_constraints_multiline_input() {
        let c = tui_layout_constraints(false, 3);
        // input_height = 3 + 2 = 5
        assert_eq!(
            c,
            vec![
                Constraint::Length(1), // title bar
                Constraint::Length(1), // workflow progress
                Constraint::Min(5),    // conversation
                Constraint::Length(1), // status hint
                Constraint::Length(5), // input (3 lines + 2 border)
            ]
        );
    }

    #[test]
    fn test_tui_layout_constraints_multiline_with_permission() {
        let c = tui_layout_constraints(true, 2);
        assert_eq!(
            c,
            vec![
                Constraint::Length(1), // title bar
                Constraint::Length(1), // workflow progress
                Constraint::Min(5),    // conversation
                Constraint::Length(2), // permission bar
                Constraint::Length(1), // status hint
                Constraint::Length(4), // input (2 lines + 2 border)
            ]
        );
    }

    #[test]
    fn test_format_token_count_small() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(500), "500");
        assert_eq!(format_token_count(999), "999");
    }

    #[test]
    fn test_format_token_count_thousands() {
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(1500), "1.5k");
        assert_eq!(format_token_count(32000), "32.0k");
        assert_eq!(format_token_count(128000), "128.0k");
    }

    #[test]
    fn test_format_token_count_millions() {
        assert_eq!(format_token_count(1_000_000), "1.0M");
        assert_eq!(format_token_count(2_500_000), "2.5M");
    }

    #[test]
    fn test_token_progress_bar_empty() {
        assert_eq!(token_progress_bar(0, 128_000), "[░░░░░░░░]");
    }

    #[test]
    fn test_token_progress_bar_half() {
        let bar = token_progress_bar(64_000, 128_000);
        assert_eq!(bar, "[████░░░░]");
    }

    #[test]
    fn test_token_progress_bar_full() {
        assert_eq!(token_progress_bar(128_000, 128_000), "[████████]");
    }

    #[test]
    fn test_token_progress_bar_over_capacity() {
        // Should clamp at 100%
        assert_eq!(token_progress_bar(200_000, 128_000), "[████████]");
    }

    #[test]
    fn test_token_progress_bar_zero_capacity() {
        assert_eq!(token_progress_bar(100, 0), "[░░░░░░░░]");
    }

    #[test]
    fn test_truncate_output_short() {
        let (text, truncated) = truncate_output("hello", 200);
        assert_eq!(text, "hello");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_output_exact() {
        let input = "a".repeat(200);
        let (text, truncated) = truncate_output(&input, 200);
        assert_eq!(text.len(), 200);
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_output_long() {
        let input = "x".repeat(300);
        let (text, truncated) = truncate_output(&input, 200);
        assert_eq!(text.len(), 200);
        assert!(truncated);
    }

    #[test]
    fn test_truncate_output_unicode_boundary() {
        // '中' is 3 bytes, so 2 chars = 6 bytes
        let input = "中文测试数据超长";
        let (text, truncated) = truncate_output(input, 6);
        assert_eq!(text, "中文");
        assert!(truncated);
    }

    #[test]
    fn test_display_verbosity_parse() {
        assert!(matches!(
            DisplayVerbosity::parse("compact"),
            Some(DisplayVerbosity::Compact)
        ));
        assert!(matches!(
            DisplayVerbosity::parse("c"),
            Some(DisplayVerbosity::Compact)
        ));
        assert!(matches!(
            DisplayVerbosity::parse("normal"),
            Some(DisplayVerbosity::Normal)
        ));
        assert!(matches!(
            DisplayVerbosity::parse("n"),
            Some(DisplayVerbosity::Normal)
        ));
        assert!(matches!(
            DisplayVerbosity::parse("verbose"),
            Some(DisplayVerbosity::Verbose)
        ));
        assert!(matches!(
            DisplayVerbosity::parse("v"),
            Some(DisplayVerbosity::Verbose)
        ));
        assert!(matches!(
            DisplayVerbosity::parse("debug"),
            Some(DisplayVerbosity::Verbose)
        ));
        assert!(DisplayVerbosity::parse("unknown").is_none());
    }

    #[test]
    fn test_display_verbosity_cycle() {
        assert!(matches!(
            DisplayVerbosity::Compact.next(),
            DisplayVerbosity::Normal
        ));
        assert!(matches!(
            DisplayVerbosity::Normal.next(),
            DisplayVerbosity::Verbose
        ));
        assert!(matches!(
            DisplayVerbosity::Verbose.next(),
            DisplayVerbosity::Compact
        ));
    }

    #[test]
    fn test_display_verbosity_label() {
        assert_eq!(DisplayVerbosity::Compact.label(), "compact");
        assert_eq!(DisplayVerbosity::Normal.label(), "normal");
        assert_eq!(DisplayVerbosity::Verbose.label(), "verbose");
    }

    #[test]
    fn test_capitalize_stage() {
        assert_eq!(capitalize_stage("planning"), "Planning");
        assert_eq!(capitalize_stage("discovery"), "Discovery");
        assert_eq!(capitalize_stage(""), "");
        assert_eq!(capitalize_stage("a"), "A");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(450), "450ms");
        assert_eq!(format_duration_ms(1500), "1.5s");
        assert_eq!(format_duration_ms(60000), "1.0m");
        assert_eq!(format_duration_ms(90000), "1.5m");
        assert_eq!(format_duration_ms(0), "0ms");
    }

    #[test]
    fn test_extract_tool_summary_shell() {
        let s = extract_tool_summary("shell", r#"{"command":"ls -la","working_dir":"."}"#);
        assert_eq!(s, "ls -la");
    }

    #[test]
    fn test_extract_tool_summary_read() {
        let s = extract_tool_summary("read", r#"{"path":"README.md"}"#);
        assert_eq!(s, "README.md");
    }

    #[test]
    fn test_extract_tool_summary_grep() {
        let s = extract_tool_summary("grep", r#"{"pattern":"fn main"}"#);
        assert_eq!(s, "fn main");
    }

    #[test]
    fn test_extract_tool_summary_unknown_tool() {
        let s = extract_tool_summary("custom_tool", r#"{"path":"src/lib.rs"}"#);
        assert_eq!(s, "src/lib.rs");
    }

    #[test]
    fn test_extract_tool_summary_no_match() {
        let s = extract_tool_summary("custom_tool", r#"{"foo":"bar"}"#);
        // Falls through to raw truncation
        assert!(!s.is_empty());
    }

    #[test]
    fn test_verbosity_env_override() {
        with_env_overrides(&[("NDC_DISPLAY_VERBOSITY", Some("normal"))], || {
            let state = ReplVisualizationState::new(false);
            assert!(matches!(state.verbosity, DisplayVerbosity::Normal));
        });
    }
}

#[cfg(test)]
mod session_split_tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn test_split_hidden_returns_full_area() {
        let area = Rect::new(0, 0, 100, 30);
        let (conv, todo) = tui_session_split(area, false);
        assert_eq!(conv, area);
        assert!(todo.is_none());
    }

    #[test]
    fn test_split_visible_wide_returns_sidebar() {
        let area = Rect::new(0, 0, 100, 30);
        let (conv, todo) = tui_session_split(area, true);
        assert!(conv.width > 0);
        let sidebar = todo.expect("sidebar should be present when wide enough");
        assert_eq!(sidebar.width, 28);
        assert_eq!(conv.width + sidebar.width, area.width);
    }

    #[test]
    fn test_split_narrow_no_sidebar() {
        let area = Rect::new(0, 0, 50, 30);
        let (conv, todo) = tui_session_split(area, true);
        assert_eq!(conv, area);
        assert!(todo.is_none());
    }
}
