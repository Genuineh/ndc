//! Layout Manager — TUI layout calculation, display utilities, and formatting.
//!
//! Extracted from `repl.rs` (SEC-S1 God Object refactoring).

use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::{
    ReplCommandCompletionState, ReplVisualizationState, TuiTheme, canonical_slash_command,
    matching_slash_commands, parse_slash_tokens, slash_argument_options,
};

pub(crate) const TUI_SCROLL_STEP: usize = 3;
pub(crate) const TIMELINE_CACHE_MAX_EVENTS: usize = 1_000;
pub(crate) const WORKFLOW_STAGE_ORDER: &[&str] = &[
    "planning",
    "discovery",
    "executing",
    "verifying",
    "completing",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayVerbosity {
    Compact,
    Normal,
    Verbose,
}

impl DisplayVerbosity {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Compact => Self::Normal,
            Self::Normal => Self::Verbose,
            Self::Verbose => Self::Compact,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Normal => "normal",
            Self::Verbose => "verbose",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "compact" | "c" => Some(Self::Compact),
            "normal" | "n" => Some(Self::Normal),
            "verbose" | "v" | "debug" => Some(Self::Verbose),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkflowOverviewMode {
    Compact,
    Verbose,
}

impl WorkflowOverviewMode {
    pub(crate) fn parse(value: Option<&str>) -> Result<Self, String> {
        let Some(raw) = value else {
            return Ok(Self::Verbose);
        };
        match raw.to_ascii_lowercase().as_str() {
            "compact" => Ok(Self::Compact),
            "verbose" => Ok(Self::Verbose),
            _ => Err("Usage: /workflow [compact|verbose]".to_string()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WorkflowOverviewMode::Compact => "compact",
            WorkflowOverviewMode::Verbose => "verbose",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TuiSessionViewState {
    pub(crate) scroll_offset: usize,
    pub(crate) auto_follow: bool,
    pub(crate) body_height: usize,
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

pub(crate) fn short_session_id(value: Option<&str>) -> String {
    let session = value.unwrap_or("-");
    let max = 12usize;
    if session.chars().count() <= max {
        return session.to_string();
    }
    let prefix = session.chars().take(max).collect::<String>();
    format!("{}…", prefix)
}

pub(crate) fn workflow_progress_descriptor(
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
pub(crate) fn build_status_line(
    status: &crate::agent_mode::AgentModeStatus,
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

pub(crate) fn tool_status_narrative(tool_name: Option<&str>) -> &'static str {
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

pub(crate) fn build_title_bar<'a>(
    status: &crate::agent_mode::AgentModeStatus,
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
        Span::styled(
            format!("{}", project_name),
            Style::default().fg(theme.primary),
        ),
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

pub(crate) fn build_workflow_progress_bar<'a>(
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
    Line::from(spans)
}

pub(crate) fn build_permission_bar<'a>(
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

pub(crate) fn build_status_hint_bar<'a>(
    input: &str,
    completion: Option<&ReplCommandCompletionState>,
    viz_state: &ReplVisualizationState,
    stream_state: &str,
    theme: &TuiTheme,
    is_processing: bool,
) -> Line<'a> {
    if let Some((_raw, norm, args, trailing_space)) = parse_slash_tokens(input) {
        if completion.is_none() {
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

pub(crate) fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub(crate) fn token_progress_bar(current: u64, capacity: u64) -> String {
    let ratio = if capacity == 0 {
        0.0
    } else {
        (current as f64 / capacity as f64).clamp(0.0, 1.0)
    };
    let filled = (ratio * 8.0).round() as usize;
    let empty = 8 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

pub(crate) fn truncate_output(text: &str, max_chars: usize) -> (String, bool) {
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

pub(crate) fn capitalize_stage(stage: &str) -> String {
    let mut chars = stage.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

pub(crate) fn extract_tool_summary(tool_name: &str, args_json: &str) -> String {
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

pub(crate) fn format_duration_ms(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{}ms", ms)
    }
}

pub(crate) fn input_line_count(input: &str) -> u16 {
    let lines = input.chars().filter(|c| *c == '\n').count() as u16 + 1;
    lines.clamp(1, 4)
}

pub(crate) fn tui_layout_constraints(has_permission: bool, input_lines: u16) -> Vec<Constraint> {
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

pub(crate) fn resolve_stream_state(
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

pub(crate) fn apply_stream_command(
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

pub(crate) fn apply_tokens_command(
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
pub(crate) fn calc_log_scroll(log_count: usize, body_height: usize) -> u16 {
    log_count.saturating_sub(body_height).min(u16::MAX as usize) as u16
}

pub(crate) fn calc_log_scroll_usize(log_count: usize, body_height: usize) -> usize {
    log_count.saturating_sub(body_height)
}

pub(crate) fn effective_log_scroll(log_count: usize, session_view: &TuiSessionViewState) -> usize {
    let max_scroll = calc_log_scroll_usize(log_count, session_view.body_height);
    if session_view.auto_follow {
        max_scroll
    } else {
        session_view.scroll_offset.min(max_scroll)
    }
}

pub(crate) fn append_timeline_events(
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
pub(crate) struct ReplRuntimeMetrics {
    pub(crate) tool_calls_total: usize,
    pub(crate) tool_calls_failed: usize,
    pub(crate) tool_duration_samples: usize,
    pub(crate) tool_duration_total_ms: u64,
    pub(crate) permission_waits: usize,
    pub(crate) error_events: usize,
}

impl ReplRuntimeMetrics {
    pub(crate) fn avg_tool_duration_ms(self) -> Option<u64> {
        if self.tool_duration_samples == 0 {
            None
        } else {
            Some(self.tool_duration_total_ms / self.tool_duration_samples as u64)
        }
    }

    pub(crate) fn tool_error_rate_percent(self) -> u64 {
        if self.tool_calls_total == 0 {
            0
        } else {
            ((self.tool_calls_failed as u64) * 100) / (self.tool_calls_total as u64)
        }
    }
}

pub(crate) fn compute_runtime_metrics(
    timeline: &[ndc_core::AgentExecutionEvent],
) -> ReplRuntimeMetrics {
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
pub(crate) struct WorkflowStageProgress {
    pub(crate) count: usize,
    pub(crate) total_ms: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowProgressSummary {
    pub(crate) stages: std::collections::BTreeMap<String, WorkflowStageProgress>,
    pub(crate) current_stage: Option<String>,
    pub(crate) current_stage_active_ms: u64,
    pub(crate) history_may_be_partial: bool,
}

pub(crate) fn compute_workflow_progress_summary(
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

pub(crate) fn group_timeline_by_stage<'a>(
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

pub(crate) fn extract_preview<'a>(message: &'a str, marker: &str) -> Option<&'a str> {
    let idx = message.find(marker)?;
    let start = idx + marker.len();
    let rest = message[start..].trim_start();
    let cut = rest.find('|').unwrap_or(rest.len());
    Some(rest[..cut].trim())
}

pub(crate) fn extract_tool_args_preview(message: &str) -> Option<&str> {
    extract_preview(message, "args_preview:")
}

pub(crate) fn extract_tool_result_preview(message: &str) -> Option<&str> {
    extract_preview(message, "result_preview:")
}
