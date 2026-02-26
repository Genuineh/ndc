//! REPL - OpenCode-style Natural Language Interaction
//!
//! 职责：
//! - OpenCode 风格的自然语言 REPL
//! - AI Agent 默认启用，直接处理所有用户输入
//! - 人类用户只需要用自然语言描述需求
//! - AI 自动理解意图并执行操作
//!
//! 设计理念 (来自 OpenCode):
//! - Agent 模式默认启用，无需手动开启
//! - 用户输入直接发送给 AI，不经过命令解析
//! - 移除 /create, /list, /run 等人类命令
//! - 简洁的 UI，专注于自然语言交互

use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};

// Agent mode integration
use crate::agent_mode::{AgentModeConfig, AgentModeManager, handle_agent_command};
use crate::redaction::{RedactionMode, sanitize_text};

const TUI_MAX_LOG_LINES: usize = 3000;
const TUI_SCROLL_STEP: usize = 3;
const TIMELINE_CACHE_MAX_EVENTS: usize = 1_000;
const AVAILABLE_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "minimax",
    "minimax-coding-plan",
    "minimax-cn",
    "minimax-cn-coding-plan",
    "openrouter",
    "ollama",
];
const WORKFLOW_STAGE_ORDER: &[&str] = &[
    "planning",
    "discovery",
    "executing",
    "verifying",
    "completing",
];

/// REPL 配置 (OpenCode 风格 - 极简)
#[derive(Debug, Clone)]
pub struct ReplConfig {
    /// 提示符
    pub prompt: String,

    /// 是否显示思考过程
    pub show_thought: bool,

    /// 会话超时（秒）
    pub session_timeout: u64,

    /// 历史文件路径
    pub history_file: PathBuf,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            prompt: "> ".to_string(),
            show_thought: false,
            session_timeout: 3600,
            history_file: PathBuf::from(".ndc/repl_history"),
        }
    }
}

impl ReplConfig {
    pub fn new(history_file: PathBuf) -> Self {
        Self {
            history_file,
            ..Self::default()
        }
    }
}

/// REPL 状态
#[derive(Debug, Clone)]
pub struct ReplState {
    /// 当前会话ID
    pub session_id: String,

    /// 最后活动时间
    pub last_activity: Instant,

    /// 当前 LLM Provider
    pub current_provider: Option<String>,

    /// 当前 LLM 模型
    pub current_model: Option<String>,
}

impl Default for ReplState {
    fn default() -> Self {
        Self {
            session_id: format!("{:x}", chrono::Utc::now().timestamp_millis()),
            last_activity: Instant::now(),
            current_provider: None,
            current_model: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ReplVisualizationState {
    show_thinking: bool,
    show_tool_details: bool,
    expand_tool_cards: bool,
    live_events_enabled: bool,
    show_usage_metrics: bool,
    verbosity: DisplayVerbosity,
    last_emitted_round: usize,
    timeline_limit: usize,
    timeline_cache: Vec<ndc_core::AgentExecutionEvent>,
    redaction_mode: RedactionMode,
    hidden_thinking_round_hints: BTreeSet<usize>,
    current_workflow_stage: Option<String>,
    current_workflow_stage_index: Option<u32>,
    current_workflow_stage_total: Option<u32>,
    current_workflow_stage_started_at: Option<chrono::DateTime<chrono::Utc>>,
    session_token_total: u64,
    latest_round_token_total: u64,
    permission_blocked: bool,
    permission_pending_message: Option<String>,
}

// ===== Theme System =====

#[derive(Debug, Clone, Copy)]
struct TuiTheme {
    text_strong: Color,
    text_base: Color,
    text_muted: Color,
    text_dim: Color,
    primary: Color,
    success: Color,
    warning: Color,
    danger: Color,
    info: Color,
    user_accent: Color,
    assistant_accent: Color,
    tool_accent: Color,
    thinking_accent: Color,
    border_normal: Color,
    border_active: Color,
    border_dim: Color,
    progress_done: Color,
    progress_active: Color,
    progress_pending: Color,
}

impl TuiTheme {
    fn default_dark() -> Self {
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

// ===== Input History =====

#[derive(Debug, Clone)]
struct InputHistory {
    entries: Vec<String>,
    cursor: Option<usize>,
    draft: String,
    max_entries: usize,
}

impl InputHistory {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            draft: String::new(),
            max_entries,
        }
    }

    fn push(&mut self, entry: String) {
        if entry.trim().is_empty() {
            return;
        }
        self.entries.retain(|e| e != &entry);
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.cursor = None;
    }

    fn up(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.cursor {
            None => {
                self.draft = current_input.to_string();
                self.cursor = Some(self.entries.len() - 1);
            }
            Some(0) => return Some(&self.entries[0]),
            Some(i) => {
                self.cursor = Some(i - 1);
            }
        }
        self.cursor.map(|i| self.entries[i].as_str())
    }

    fn down(&mut self) -> Option<&str> {
        match self.cursor {
            None => None,
            Some(i) if i + 1 >= self.entries.len() => {
                self.cursor = None;
                Some(self.draft.as_str())
            }
            Some(i) => {
                self.cursor = Some(i + 1);
                Some(self.entries[i + 1].as_str())
            }
        }
    }

    fn reset(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }
}

#[derive(Debug, Clone)]
struct ReplTuiKeymap {
    toggle_thinking: char,
    toggle_details: char,
    toggle_tool_cards: char,
    show_recent_thinking: char,
    show_timeline: char,
    clear_panel: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TuiShortcutAction {
    ToggleThinking,
    ToggleDetails,
    ToggleToolCards,
    ShowRecentThinking,
    ShowTimeline,
    ClearPanel,
}

/// Controls how much detail is shown for process events in the conversation panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayVerbosity {
    /// Minimal output: single-line stage, hide steps/tokens, tool one-liner
    Compact,
    /// Moderate output: stage + detail, formatted tokens, tool with params
    Normal,
    /// Full debug output: all raw messages, JSON args, meta fields
    Verbose,
}

impl DisplayVerbosity {
    fn next(self) -> Self {
        match self {
            Self::Compact => Self::Normal,
            Self::Normal => Self::Verbose,
            Self::Verbose => Self::Compact,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Normal => "normal",
            Self::Verbose => "verbose",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "compact" | "c" => Some(Self::Compact),
            "normal" | "n" => Some(Self::Normal),
            "verbose" | "v" | "debug" => Some(Self::Verbose),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkflowOverviewMode {
    Compact,
    Verbose,
}

impl WorkflowOverviewMode {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        let Some(raw) = value else {
            return Ok(Self::Verbose);
        };
        match raw.to_ascii_lowercase().as_str() {
            "compact" => Ok(Self::Compact),
            "verbose" => Ok(Self::Verbose),
            _ => Err("Usage: /workflow [compact|verbose]".to_string()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            WorkflowOverviewMode::Compact => "compact",
            WorkflowOverviewMode::Verbose => "verbose",
        }
    }
}

// ===== Chat Turn Model (P1-UX-2) =====

/// Status of a tool call card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCardStatus {
    Running,
    Completed,
    Failed,
}

/// A collapsible card representing a tool call execution.
#[derive(Debug, Clone)]
struct ToolCallCard {
    name: String,
    status: ToolCardStatus,
    duration: Option<String>,
    args_summary: Option<String>,
    output_preview: Option<String>,
    is_error: bool,
    collapsed: bool,
    round: usize,
}

/// A single structured entry in the conversation log.
#[derive(Debug, Clone)]
enum ChatEntry {
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

/// A complete conversation turn grouping a user message and the agent's
/// response cycle (events + assistant reply).
#[derive(Debug, Clone)]
struct ChatTurn {
    turn_id: usize,
    entries: Vec<ChatEntry>,
}

const TUI_MAX_CHAT_ENTRIES: usize = 3000;

fn push_chat_entry(entries: &mut Vec<ChatEntry>, entry: ChatEntry) {
    entries.push(entry);
    if entries.len() > TUI_MAX_CHAT_ENTRIES {
        let overflow = entries.len() - TUI_MAX_CHAT_ENTRIES;
        entries.drain(0..overflow);
    }
}

fn push_chat_entries(entries: &mut Vec<ChatEntry>, new_entries: Vec<ChatEntry>) {
    for entry in new_entries {
        push_chat_entry(entries, entry);
    }
}

/// Count the number of rendered display lines a single ChatEntry will produce.
fn chat_entry_display_lines(entry: &ChatEntry) -> usize {
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
fn total_display_lines(entries: &[ChatEntry]) -> usize {
    entries.iter().map(chat_entry_display_lines).sum()
}

/// Render structured chat entries to styled ratatui Lines.
fn style_chat_entries(entries: &[ChatEntry]) -> Vec<Line<'static>> {
    let theme = TuiTheme::default_dark();
    let mut lines = Vec::new();
    for entry in entries {
        style_chat_entry(entry, &theme, &mut lines);
    }
    lines
}

/// Render a single ChatEntry into styled Lines.
fn style_chat_entry(entry: &ChatEntry, theme: &TuiTheme, lines: &mut Vec<Line<'static>>) {
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
fn event_to_entries(
    event: &ndc_core::AgentExecutionEvent,
    viz_state: &mut ReplVisualizationState,
) -> Vec<ChatEntry> {
    if !matches!(
        event.kind,
        ndc_core::AgentExecutionEventKind::PermissionAsked
            | ndc_core::AgentExecutionEventKind::Reasoning
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
        ndc_core::AgentExecutionEventKind::WorkflowStage => {
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
        ndc_core::AgentExecutionEventKind::Reasoning => {
            let collapsed = !viz_state.show_thinking;
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
        ndc_core::AgentExecutionEventKind::ToolCallStart => {
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
                args_summary: args_summary,
                output_preview: None,
                is_error: false,
                collapsed: !viz_state.expand_tool_cards,
                round: event.round,
            }));
        }
        ndc_core::AgentExecutionEventKind::ToolCallEnd => {
            let tool = event.tool_name.as_deref().unwrap_or("unknown");
            let duration = event.duration_ms.map(|d| format_duration_ms(d));
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
                round: event.round,
            }));
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
        ndc_core::AgentExecutionEventKind::PermissionAsked => {
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
        ndc_core::AgentExecutionEventKind::StepStart
        | ndc_core::AgentExecutionEventKind::StepFinish
        | ndc_core::AgentExecutionEventKind::Verification => match v {
            DisplayVerbosity::Compact => {}
            DisplayVerbosity::Normal => {
                if matches!(event.kind, ndc_core::AgentExecutionEventKind::StepFinish)
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
                    && matches!(event.kind, ndc_core::AgentExecutionEventKind::StepStart)
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
        ndc_core::AgentExecutionEventKind::Error => {
            entries.push(ChatEntry::ErrorNote(format!(
                "[Error][r{}] {}",
                event.round,
                sanitize_text(&event.message, viz_state.redaction_mode)
            )));
        }
        ndc_core::AgentExecutionEventKind::SessionStatus
        | ndc_core::AgentExecutionEventKind::Text => {}
    }
    entries
}

/// Drain live execution events into structured chat entries.
fn drain_live_chat_entries(
    receiver: &mut Option<tokio::sync::broadcast::Receiver<ndc_core::AgentSessionExecutionEvent>>,
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
fn effective_chat_scroll(entries: &[ChatEntry], view: &TuiSessionViewState) -> usize {
    let total = total_display_lines(entries);
    if view.auto_follow || total <= view.body_height {
        total.saturating_sub(view.body_height)
    } else {
        view.scroll_offset
            .min(total.saturating_sub(view.body_height))
    }
}

/// Toggle collapse state for all tool cards in entries.
fn toggle_all_tool_cards(entries: &mut [ChatEntry]) {
    for entry in entries.iter_mut() {
        if let ChatEntry::ToolCard(card) = entry {
            card.collapsed = !card.collapsed;
        }
    }
}

/// Toggle collapse state for all reasoning blocks in entries.
fn toggle_all_reasoning_blocks(entries: &mut [ChatEntry]) {
    for entry in entries.iter_mut() {
        if let ChatEntry::ReasoningBlock { collapsed, .. } = entry {
            *collapsed = !*collapsed;
        }
    }
}

/// Bridge function: push a plain text string as a typed ChatEntry.
/// Empty text becomes Separator; "[Error]" prefix becomes ErrorNote;
/// "[Warning]" or "[Tip]" becomes WarningNote; everything else SystemNote.
fn push_text_entry(entries: &mut Vec<ChatEntry>, text: &str) {
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
fn entries_to_plain_text(entries: &[ChatEntry]) -> String {
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

#[derive(Debug, Clone)]
struct TuiSessionViewState {
    scroll_offset: usize,
    auto_follow: bool,
    body_height: usize,
}

#[derive(Debug, Clone)]
struct ReplCommandCompletionState {
    suggestions: Vec<String>,
    selected_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct SlashCommandSpec {
    command: &'static str,
    summary: &'static str,
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

const SLASH_COMMAND_SPECS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        command: "/help",
        summary: "show help",
    },
    SlashCommandSpec {
        command: "/provider",
        summary: "switch provider",
    },
    SlashCommandSpec {
        command: "/providers",
        summary: "alias of /provider",
    },
    SlashCommandSpec {
        command: "/model",
        summary: "switch model",
    },
    SlashCommandSpec {
        command: "/agent",
        summary: "agent controls",
    },
    SlashCommandSpec {
        command: "/status",
        summary: "show status",
    },
    SlashCommandSpec {
        command: "/thinking",
        summary: "toggle thinking",
    },
    SlashCommandSpec {
        command: "/details",
        summary: "toggle details",
    },
    SlashCommandSpec {
        command: "/cards",
        summary: "toggle tool cards",
    },
    SlashCommandSpec {
        command: "/verbosity",
        summary: "compact/normal/verbose",
    },
    SlashCommandSpec {
        command: "/stream",
        summary: "stream on/off/status",
    },
    SlashCommandSpec {
        command: "/workflow",
        summary: "workflow overview",
    },
    SlashCommandSpec {
        command: "/tokens",
        summary: "usage metrics",
    },
    SlashCommandSpec {
        command: "/metrics",
        summary: "runtime metrics",
    },
    SlashCommandSpec {
        command: "/timeline",
        summary: "show timeline",
    },
    SlashCommandSpec {
        command: "/clear",
        summary: "clear panel",
    },
    SlashCommandSpec {
        command: "/copy",
        summary: "save session to file",
    },
    SlashCommandSpec {
        command: "/resume",
        summary: "resume latest session",
    },
    SlashCommandSpec {
        command: "/new",
        summary: "start new session",
    },
    SlashCommandSpec {
        command: "/session",
        summary: "list sessions for current project",
    },
    SlashCommandSpec {
        command: "/project",
        summary: "list or switch project",
    },
    SlashCommandSpec {
        command: "/exit",
        summary: "exit repl",
    },
];

impl ReplTuiKeymap {
    fn from_env() -> Self {
        Self {
            toggle_thinking: env_char("NDC_REPL_KEY_TOGGLE_THINKING", 't'),
            toggle_details: env_char("NDC_REPL_KEY_TOGGLE_DETAILS", 'd'),
            toggle_tool_cards: env_char("NDC_REPL_KEY_TOGGLE_TOOL_CARDS", 'e'),
            show_recent_thinking: env_char("NDC_REPL_KEY_SHOW_RECENT_THINKING", 'y'),
            show_timeline: env_char("NDC_REPL_KEY_SHOW_TIMELINE", 'i'),
            clear_panel: env_char("NDC_REPL_KEY_CLEAR_PANEL", 'l'),
        }
    }

    fn hint(&self) -> String {
        format!(
            "Ctrl+{} thinking, Ctrl+{} details, Ctrl+{} cards, Ctrl+{} recent thinking, Ctrl+{} timeline, Ctrl+{} clear",
            self.toggle_thinking.to_ascii_uppercase(),
            self.toggle_details.to_ascii_uppercase(),
            self.toggle_tool_cards.to_ascii_uppercase(),
            self.show_recent_thinking.to_ascii_uppercase(),
            self.show_timeline.to_ascii_uppercase(),
            self.clear_panel.to_ascii_uppercase()
        )
    }
}

impl ReplVisualizationState {
    fn new(show_thinking: bool) -> Self {
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

fn env_bool(key: &str) -> Option<bool> {
    let value = std::env::var(key).ok()?.to_lowercase();
    match value.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse::<usize>().ok()
}

fn env_char(key: &str, default: char) -> char {
    std::env::var(key)
        .ok()
        .and_then(|v| v.chars().next())
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_alphanumeric())
        .unwrap_or(default)
}

fn key_is_ctrl_char(key: &crossterm::event::KeyEvent, ch: char) -> bool {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    match key.code {
        KeyCode::Char(c) => c.eq_ignore_ascii_case(&ch),
        _ => false,
    }
}

fn detect_tui_shortcut(
    key: &crossterm::event::KeyEvent,
    keymap: &ReplTuiKeymap,
) -> Option<TuiShortcutAction> {
    if key_is_ctrl_char(key, keymap.toggle_thinking) {
        return Some(TuiShortcutAction::ToggleThinking);
    }
    if key_is_ctrl_char(key, keymap.toggle_details) {
        return Some(TuiShortcutAction::ToggleDetails);
    }
    if key_is_ctrl_char(key, keymap.toggle_tool_cards) {
        return Some(TuiShortcutAction::ToggleToolCards);
    }
    if key_is_ctrl_char(key, keymap.show_recent_thinking) {
        return Some(TuiShortcutAction::ShowRecentThinking);
    }
    if key_is_ctrl_char(key, keymap.show_timeline) {
        return Some(TuiShortcutAction::ShowTimeline);
    }
    if key_is_ctrl_char(key, keymap.clear_panel) {
        return Some(TuiShortcutAction::ClearPanel);
    }
    None
}

fn canonical_slash_command(command: &str) -> &str {
    match command {
        "/providers" | "/provider" | "/p" => "/provider",
        "/thinking" | "/t" => "/thinking",
        "/details" | "/d" => "/details",
        "/status" | "/st" => "/status",
        "/help" | "/h" => "/help",
        "/cards" | "/toolcards" => "/cards",
        "/verbosity" | "/v" => "/verbosity",
        "/clear" | "/cls" => "/clear",
        "/resume" | "/r" => "/resume",
        _ => command,
    }
}

fn slash_argument_options(command: &str) -> Option<&'static [&'static str]> {
    match canonical_slash_command(command) {
        "/provider" => Some(AVAILABLE_PROVIDERS),
        "/stream" => Some(&["on", "off", "status"]),
        "/workflow" => Some(&["compact", "verbose"]),
        "/thinking" => Some(&["show", "now"]),
        "/tokens" => Some(&["show", "hide", "reset", "status"]),
        "/verbosity" => Some(&["compact", "normal", "verbose"]),
        _ => None,
    }
}

fn parse_slash_tokens(input: &str) -> Option<(String, String, Vec<String>, bool)> {
    let raw = input.trim_start();
    if !raw.starts_with('/') {
        return None;
    }
    let trailing_space = raw.ends_with(' ');
    let mut iter = raw.split_whitespace();
    let command_raw = iter.next().unwrap_or("/").to_string();
    let command_norm = command_raw.to_ascii_lowercase();
    let args = iter.map(|value| value.to_string()).collect::<Vec<_>>();
    Some((command_raw, command_norm, args, trailing_space))
}

fn matching_slash_commands(prefix: &str) -> Vec<SlashCommandSpec> {
    let normalized = prefix.trim();
    if normalized.is_empty() || normalized == "/" {
        return SLASH_COMMAND_SPECS.to_vec();
    }
    SLASH_COMMAND_SPECS
        .iter()
        .copied()
        .filter(|spec| spec.command.starts_with(normalized))
        .collect()
}

fn completion_suggestions_for_input(input: &str) -> Vec<String> {
    let Some((command_raw, command_norm, args, trailing_space)) = parse_slash_tokens(input) else {
        return Vec::new();
    };
    if args.is_empty() && !trailing_space {
        return matching_slash_commands(command_norm.as_str())
            .into_iter()
            .map(|spec| spec.command.to_string())
            .collect();
    }
    if args.len() > 1 {
        return Vec::new();
    }
    let arg_prefix = args.first().map(String::as_str).unwrap_or("");
    let arg_prefix = arg_prefix.to_ascii_lowercase();
    slash_argument_options(command_norm.as_str())
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|option| option.starts_with(arg_prefix.as_str()))
        .map(|option| format!("{} {}", command_raw, option))
        .collect()
}

#[cfg(test)]
fn build_input_hint_lines(
    input: &str,
    completion: Option<&ReplCommandCompletionState>,
) -> Vec<String> {
    let Some((_command_raw, command_norm, args, trailing_space)) = parse_slash_tokens(input) else {
        return vec!["Type '/' to open command hints. Use Tab/Shift+Tab to complete.".to_string()];
    };
    let selected = completion.and_then(|state| {
        state.suggestions.get(state.selected_index).map(|value| {
            (
                state.selected_index + 1,
                state.suggestions.len(),
                value.clone(),
            )
        })
    });

    if args.is_empty() && !trailing_space {
        let matches = matching_slash_commands(command_norm.as_str());
        if matches.is_empty() {
            return vec!["No command match. Try /help.".to_string()];
        }
        let items = matches
            .iter()
            .map(|spec| format!("{} ({})", spec.command, spec.summary))
            .collect::<Vec<_>>()
            .join(" | ");
        let mut lines = vec![
            format!("Commands [{}]: {}", matches.len(), items),
            "Tab next | Shift+Tab previous".to_string(),
        ];
        if let Some((index, total, value)) = selected {
            lines.push(format!("Selected [{}/{}]: {}", index, total, value));
        }
        return lines;
    }

    if args.len() > 1 {
        return vec!["Parameter hints are available for first argument only.".to_string()];
    }

    let arg_prefix = args.first().cloned().unwrap_or_default();
    let options = slash_argument_options(command_norm.as_str());
    if let Some(options) = options {
        let all_options = options.join(", ");
        let filtered = if arg_prefix.is_empty() {
            options.to_vec()
        } else {
            options
                .iter()
                .copied()
                .filter(|option| option.starts_with(arg_prefix.to_ascii_lowercase().as_str()))
                .collect::<Vec<_>>()
        };
        let mut lines = vec![
            format!(
                "{} options: {}",
                canonical_slash_command(command_norm.as_str()),
                all_options
            ),
            if arg_prefix.is_empty() {
                "Type argument or use Tab/Shift+Tab to choose.".to_string()
            } else if filtered.is_empty() {
                format!("No option match for '{}'.", arg_prefix)
            } else {
                format!("Matched [{}]: {}", filtered.len(), filtered.join(", "))
            },
        ];
        if let Some((index, total, value)) = selected {
            lines.push(format!("Selected [{}/{}]: {}", index, total, value));
        }
        return lines;
    }

    let canonical = canonical_slash_command(command_norm.as_str());
    let mut lines = vec![format!(
        "{}: {}",
        canonical,
        SLASH_COMMAND_SPECS
            .iter()
            .find(|spec| spec.command == canonical)
            .map(|spec| spec.summary)
            .unwrap_or("no predefined hint")
    )];
    match canonical {
        "/timeline" => lines.push("Usage: /timeline 40".to_string()),
        "/model" => {
            lines.push("Usage: /model <model-id> or /model <provider>/<model-id>".to_string())
        }
        _ => lines.push("No predefined parameter options for this command.".to_string()),
    }
    lines
}

fn apply_slash_completion(
    input: &mut String,
    completion: &mut Option<ReplCommandCompletionState>,
    reverse: bool,
) -> bool {
    if let Some(state) = completion.as_mut()
        && !state.suggestions.is_empty()
        && state.selected_index < state.suggestions.len()
        && input.trim() == state.suggestions[state.selected_index]
    {
        let len = state.suggestions.len();
        state.selected_index = if reverse {
            if state.selected_index == 0 {
                len - 1
            } else {
                state.selected_index - 1
            }
        } else {
            (state.selected_index + 1) % len
        };
        *input = state.suggestions[state.selected_index].clone();
        return true;
    }

    let suggestions = completion_suggestions_for_input(input);
    if suggestions.is_empty() {
        *completion = None;
        return false;
    }
    let selected_index = if reverse { suggestions.len() - 1 } else { 0 };
    *input = suggestions[selected_index].clone();
    *completion = Some(ReplCommandCompletionState {
        suggestions,
        selected_index,
    });
    true
}

impl ReplState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        self.last_activity.elapsed() > Duration::from_secs(timeout_secs)
    }
}

/// 运行 REPL (OpenCode 风格)
pub async fn run_repl(history_file: PathBuf, executor: Arc<ndc_runtime::Executor>) {
    let config = ReplConfig::new(history_file);
    let mut viz_state = ReplVisualizationState::new(config.show_thought);

    // 创建 Agent Mode Manager (OpenCode 风格: 默认启用)
    let agent_manager = Arc::new(AgentModeManager::new(
        executor.clone(),
        Arc::new(ndc_runtime::create_default_tool_registry_with_storage(
            executor.context().storage.clone(),
        )),
    ));

    // 启动时自动启用 Agent 模式
    let agent_config = AgentModeConfig::default();
    if let Err(e) = agent_manager.enable(agent_config).await {
        println!("[Warning] Failed to enable agent mode: {}", e);
    }

    if io::stdout().is_terminal() && std::env::var("NDC_REPL_LEGACY").is_err() {
        if let Err(e) = run_repl_tui(&config, &mut viz_state, agent_manager.clone()).await {
            warn!("TUI mode failed, fallback to legacy REPL: {}", e);
        } else {
            return;
        }
    }

    // 打印欢迎信息 (OpenCode 风格: 极简)
    println!(
        r#"
NDC - Neo Development Companion

Natural language AI assistant. Just describe what you want.

Examples:
  "Create a REST API for user management"
  "Fix the bug in the login function"
  "Run tests for the authentication module"

Commands: /help, /provider, /model, /agent, /status, /stream, /workflow, /tokens, /metrics, /clear, exit
"#
    );

    // REPL 循环
    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("{}", config.prompt);
        io::stdout().flush().unwrap();

        input.clear();

        match stdin.lock().read_line(&mut input) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let input = input.trim();
                if input.is_empty() {
                    continue;
                }

                // 处理退出
                if input == "exit" || input == "quit" || input == "q" {
                    println!("Goodbye!");
                    break;
                }

                // 处理命令
                if input.starts_with('/') {
                    handle_command(input, &config, &mut viz_state, agent_manager.clone()).await;
                } else {
                    // 自然语言输入 - 直接发送给 AI Agent
                    handle_agent_dialogue(input, &agent_manager, &mut viz_state).await;
                }
            }
            Err(e) => {
                warn!("Read error: {}", e);
                break;
            }
        }
    }

    info!("REPL exited");
}

fn short_session_id(value: Option<&str>) -> String {
    let session = value.unwrap_or("-");
    let max = 12usize;
    if session.chars().count() <= max {
        return session.to_string();
    }
    let prefix = session.chars().take(max).collect::<String>();
    format!("{}…", prefix)
}

fn workflow_progress_descriptor(
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
fn build_status_line(
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

// ===== New Layout Rendering Functions =====

fn tool_status_narrative(tool_name: Option<&str>) -> &'static str {
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

fn build_title_bar<'a>(
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

fn build_workflow_progress_bar<'a>(
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

fn build_permission_bar<'a>(viz_state: &ReplVisualizationState, theme: &TuiTheme) -> Vec<Line<'a>> {
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
            "   [y] Allow  [n] Deny  [a] Always allow",
            Style::default().fg(theme.text_muted),
        )]),
    ]
}

fn build_status_hint_bar<'a>(
    input: &str,
    completion: Option<&ReplCommandCompletionState>,
    viz_state: &ReplVisualizationState,
    stream_state: &str,
    theme: &TuiTheme,
) -> Line<'a> {
    // If typing a slash command and no completion yet, show matching commands
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

    // Show active completion
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

    // Default: compact status bar
    let sep = Span::styled(" │ ", Style::default().fg(theme.border_dim));
    let mut spans = vec![
        Span::styled(" /help", Style::default().fg(theme.text_muted)),
        sep.clone(),
    ];

    if viz_state.show_usage_metrics && viz_state.session_token_total > 0 {
        let round = viz_state.latest_round_token_total;
        let session = viz_state.session_token_total;
        // Visual token bar (8 chars wide)
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
    spans.push(Span::styled(
        "Shift+Enter newline  ↑↓ history  PgUp/Dn scroll  Esc exit",
        Style::default().fg(theme.text_dim),
    ));

    Line::from(spans)
}

fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn token_progress_bar(current: u64, capacity: u64) -> String {
    let ratio = if capacity == 0 {
        0.0
    } else {
        (current as f64 / capacity as f64).clamp(0.0, 1.0)
    };
    let filled = (ratio * 8.0).round() as usize;
    let empty = 8 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Truncate output text to `max_chars`, returning (display_text, was_truncated).
fn truncate_output(text: &str, max_chars: usize) -> (String, bool) {
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

/// Capitalize a workflow stage name: "planning" → "Planning"
fn capitalize_stage(stage: &str) -> String {
    let mut chars = stage.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Extract a human-readable one-line summary from tool name + JSON args string.
/// e.g. shell → `ls -la`, read → `src/main.rs`, write → `output.txt`
fn extract_tool_summary(tool_name: &str, args_json: &str) -> String {
    // Try to extract the key field from JSON-ish text
    let extract_field = |field: &str| -> Option<String> {
        let key = format!("\"{}\":", field);
        let idx = args_json.find(&key)?;
        let rest = &args_json[idx + key.len()..];
        let rest = rest.trim_start();
        if rest.starts_with('"') {
            let inner = &rest[1..];
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
            // Fallback: show first string value found
            if let Some(val) = extract_field("path")
                .or_else(|| extract_field("command"))
                .or_else(|| extract_field("query"))
            {
                val
            } else {
                // Ultra-fallback: truncated raw
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

/// Format a duration_ms as a human-readable string
fn format_duration_ms(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{}ms", ms)
    }
}

fn input_line_count(input: &str) -> u16 {
    let lines = input.chars().filter(|c| *c == '\n').count() as u16 + 1;
    lines.clamp(1, 4)
}

fn tui_layout_constraints(has_permission: bool, input_lines: u16) -> Vec<Constraint> {
    let input_height = input_lines + 2; // +2 for border
    let mut c = vec![
        Constraint::Length(1), // title bar
        Constraint::Length(1), // workflow progress
        Constraint::Min(5),    // conversation area
    ];
    if has_permission {
        c.push(Constraint::Length(2)); // permission bar
    }
    c.push(Constraint::Length(1)); // status hint bar
    c.push(Constraint::Length(input_height)); // input area
    c
}

fn resolve_stream_state(
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

fn apply_stream_command(
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

fn apply_tokens_command(
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

// Old 4-zone layout removed — replaced by dynamic tui_layout_constraints(has_permission)

#[cfg(test)]
fn calc_log_scroll(log_count: usize, body_height: usize) -> u16 {
    log_count.saturating_sub(body_height).min(u16::MAX as usize) as u16
}

fn calc_log_scroll_usize(log_count: usize, body_height: usize) -> usize {
    log_count.saturating_sub(body_height)
}

fn effective_log_scroll(log_count: usize, session_view: &TuiSessionViewState) -> usize {
    let max_scroll = calc_log_scroll_usize(log_count, session_view.body_height);
    if session_view.auto_follow {
        max_scroll
    } else {
        session_view.scroll_offset.min(max_scroll)
    }
}

fn move_session_scroll(session_view: &mut TuiSessionViewState, log_count: usize, delta: isize) {
    let max_scroll = calc_log_scroll_usize(log_count, session_view.body_height);
    let current = effective_log_scroll(log_count, session_view) as isize;
    let next = (current + delta).clamp(0, max_scroll as isize) as usize;
    session_view.scroll_offset = next;
    session_view.auto_follow = next >= max_scroll;
}

fn handle_session_scroll_key(
    key: &KeyEvent,
    session_view: &mut TuiSessionViewState,
    log_count: usize,
) -> bool {
    let page = (session_view.body_height / 2).max(1) as isize;
    match key.code {
        KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_session_scroll(session_view, log_count, -1);
            true
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_session_scroll(session_view, log_count, 1);
            true
        }
        KeyCode::PageUp => {
            move_session_scroll(session_view, log_count, -page);
            true
        }
        KeyCode::PageDown => {
            move_session_scroll(session_view, log_count, page);
            true
        }
        KeyCode::Home => {
            session_view.scroll_offset = 0;
            session_view.auto_follow = false;
            true
        }
        KeyCode::End => {
            session_view.scroll_offset = calc_log_scroll_usize(log_count, session_view.body_height);
            session_view.auto_follow = true;
            true
        }
        _ => false,
    }
}

fn handle_session_scroll_mouse(
    mouse: &MouseEvent,
    session_view: &mut TuiSessionViewState,
    log_count: usize,
) -> bool {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            move_session_scroll(session_view, log_count, -(TUI_SCROLL_STEP as isize));
            true
        }
        MouseEventKind::ScrollDown => {
            move_session_scroll(session_view, log_count, TUI_SCROLL_STEP as isize);
            true
        }
        _ => false,
    }
}

fn style_session_log_lines(logs: &[String]) -> Vec<Line<'static>> {
    let theme = TuiTheme::default_dark();
    logs.iter()
        .map(|line| style_session_log_line(line, &theme))
        .collect()
}

fn style_session_log_line(line: &str, theme: &TuiTheme) -> Line<'static> {
    let plain = || Line::from(Span::raw(line.to_string()));
    let muted = Style::default().fg(theme.text_muted);
    let subtle = Style::default().fg(theme.text_base);
    let title = Style::default()
        .fg(theme.assistant_accent)
        .add_modifier(Modifier::BOLD);
    let success = Style::default()
        .fg(theme.success)
        .add_modifier(Modifier::BOLD);
    let warning = Style::default()
        .fg(theme.warning)
        .add_modifier(Modifier::BOLD);
    let danger = Style::default()
        .fg(theme.danger)
        .add_modifier(Modifier::BOLD);

    if line == "You:" {
        return Line::from(vec![
            Span::styled("▌ ", Style::default().fg(theme.user_accent)),
            Span::styled(
                "You",
                Style::default()
                    .fg(theme.user_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if line == "Assistant:" {
        return Line::from(vec![
            Span::styled("▌ ", Style::default().fg(theme.assistant_accent)),
            Span::styled("Assistant", title),
        ]);
    }
    if let Some(text) = line.strip_prefix("You: ") {
        return Line::from(vec![
            Span::styled("▌ ", Style::default().fg(theme.user_accent)),
            Span::styled(
                "You  ",
                Style::default()
                    .fg(theme.user_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(text.to_string(), Style::default().fg(theme.text_strong)),
        ]);
    }
    if line.starts_with("[Agent] processing") {
        return Line::from(vec![
            Span::styled("  ◆ ", Style::default().fg(theme.warning)),
            Span::styled(
                line.strip_prefix("[Agent] ").unwrap_or(line).to_string(),
                Style::default().fg(theme.warning),
            ),
        ]);
    }
    if line.starts_with("[Error]") || line.starts_with("[Error][") {
        return Line::from(vec![
            Span::styled("  ✗ ", danger),
            Span::styled(
                line.strip_prefix("[Error] ").unwrap_or(line).to_string(),
                Style::default().fg(theme.danger),
            ),
        ]);
    }
    if line.starts_with("[Permission]") {
        return Line::from(vec![
            Span::styled("  ⚠ ", warning),
            Span::styled(
                line.strip_prefix("[Permission] ")
                    .unwrap_or(line)
                    .to_string(),
                Style::default().fg(theme.warning),
            ),
        ]);
    }
    // ── New verbosity-aware line formats ──
    if let Some(rest) = line.strip_prefix("[RoundSep] ") {
        return Line::from(Span::styled(
            format!("  {}", rest),
            Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::DIM),
        ));
    }
    if let Some(rest) = line.strip_prefix("[Stage] ") {
        return Line::from(vec![
            Span::styled("  ◆ ", Style::default().fg(theme.primary)),
            Span::styled(
                rest.to_string(),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if let Some(rest) = line.strip_prefix("[ToolRun] ") {
        return Line::from(vec![
            Span::styled("  ▸ ", Style::default().fg(theme.tool_accent)),
            Span::styled(rest.to_string(), Style::default().fg(theme.tool_accent)),
        ]);
    }
    if let Some(rest) = line.strip_prefix("[ToolEnd] ") {
        let (icon, style) = if rest.starts_with('✗') {
            ("  ✗ ", danger)
        } else {
            ("  ✓ ", success)
        };
        // Skip the icon character + space from the rest
        let body = if rest.len() > 2 { &rest[4..] } else { rest };
        return Line::from(vec![
            Span::styled(icon, style),
            Span::styled(body.to_string(), style),
        ]);
    }
    if let Some(rest) = line.strip_prefix("[PermBlock] ") {
        return Line::from(vec![
            Span::styled("  ⚠ ", warning),
            Span::styled(rest.to_string(), Style::default().fg(theme.warning)),
        ]);
    }
    if let Some(rest) = line.strip_prefix("[PermHint] ") {
        return Line::from(Span::styled(
            format!("    {}", rest),
            Style::default().fg(theme.text_muted),
        ));
    }
    if line.starts_with("[Tool]") {
        let (icon, style) = if line.contains(" failed ") {
            ("  ✗ ", danger)
        } else if line.contains(" done ") {
            ("  ✓ ", success)
        } else {
            ("  ▸ ", Style::default().fg(theme.tool_accent))
        };
        return Line::from(vec![
            Span::styled(icon, style),
            Span::styled(
                line.strip_prefix("[Tool]").unwrap_or(line).to_string(),
                style,
            ),
        ]);
    }
    if line.starts_with("[Workflow]") {
        return Line::from(vec![
            Span::styled("  ◇ ", Style::default().fg(theme.primary)),
            Span::styled(
                line.strip_prefix("[Workflow] ").unwrap_or(line).to_string(),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if line.starts_with("[Usage]") {
        return Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                line.strip_prefix("[Usage] ").unwrap_or(line).to_string(),
                Style::default().fg(theme.success),
            ),
        ]);
    }
    if line.starts_with("[Thinking]") {
        return Line::from(vec![
            Span::styled("  ◌ ", Style::default().fg(theme.thinking_accent)),
            Span::styled(
                line.strip_prefix("[Thinking] ").unwrap_or(line).to_string(),
                Style::default().fg(theme.thinking_accent),
            ),
        ]);
    }
    if line.starts_with("[Step]") {
        return Line::from(vec![
            Span::styled("  → ", Style::default().fg(theme.primary)),
            Span::styled(
                line.strip_prefix("[Step] ").unwrap_or(line).to_string(),
                Style::default().fg(theme.primary),
            ),
        ]);
    }
    if line.trim_start().starts_with("[stage:") {
        return Line::from(vec![
            Span::styled("  ◆ ", Style::default().fg(theme.primary)),
            Span::styled(
                line.to_string(),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if line.starts_with("[Agent][") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(theme.warning),
        ));
    }
    if line.starts_with("[OK]") {
        return Line::from(vec![
            Span::styled("  ✓ ", success),
            Span::styled(
                line.strip_prefix("[OK] ").unwrap_or(line).to_string(),
                Style::default().fg(theme.success),
            ),
        ]);
    }
    if line.starts_with("[Tip]") || line.starts_with("[Warning]") {
        return Line::from(Span::styled(line.to_string(), warning));
    }
    if line.starts_with("Recent Thinking") || line.starts_with("Recent Execution Timeline") {
        return Line::from(Span::styled(line.to_string(), title));
    }
    if line.starts_with("  - r") {
        return Line::from(Span::styled(line.to_string(), subtle));
    }
    if line.starts_with("  (") {
        return Line::from(Span::styled(line.to_string(), muted));
    }
    if let Some(value) = line.strip_prefix("  ├─ input : ") {
        return Line::from(vec![
            Span::styled("  ├─ ", subtle),
            Span::styled(
                "input",
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" : ", subtle),
            Span::raw(value.to_string()),
        ]);
    }
    if let Some(value) = line.strip_prefix("  ├─ output: ") {
        let (display, truncated) = truncate_output(value, 200);
        let mut spans = vec![
            Span::styled("  ├─ ", subtle),
            Span::styled(
                "output",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(": ", subtle),
            Span::raw(display),
        ];
        if truncated {
            spans.push(Span::styled(
                " … (truncated)",
                Style::default().fg(theme.text_dim),
            ));
        }
        return Line::from(spans);
    }
    if let Some(value) = line.strip_prefix("  ├─ error : ") {
        return Line::from(vec![
            Span::styled("  ├─ ", subtle),
            Span::styled(
                "error",
                Style::default()
                    .fg(theme.danger)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" : ", subtle),
            Span::raw(value.to_string()),
        ]);
    }
    if let Some(value) = line.strip_prefix("  └─ meta  : ") {
        return Line::from(vec![
            Span::styled("  └─ ", subtle),
            Span::styled(
                "meta",
                Style::default()
                    .fg(theme.thinking_accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  : ", subtle),
            Span::raw(value.to_string()),
        ]);
    }
    if line.starts_with("  └─ (collapsed card") {
        return Line::from(Span::styled(line.to_string(), muted));
    }
    if line.starts_with("Shortcuts:") {
        return Line::from(Span::styled(line.to_string(), muted));
    }
    // Simple Markdown rendering for assistant output (indented lines)
    if line.starts_with("  ")
        && !line.starts_with("  ├")
        && !line.starts_with("  └")
        && !line.starts_with("  (")
        && !line.starts_with("  - r")
    {
        return render_inline_markdown(line, theme);
    }
    plain()
}

/// Render simple inline Markdown: `code`, **bold**, *italic*, # headers, - bullets
fn render_inline_markdown<'a>(line: &str, theme: &TuiTheme) -> Line<'a> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    // Code fence lines (``` language)
    if trimmed.starts_with("```") {
        return Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(trimmed.to_string(), Style::default().fg(theme.text_muted)),
        ]);
    }

    // Headers (# ## ###)
    if let Some(rest) = trimmed.strip_prefix("### ") {
        return Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(
                format!("   {}", rest),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        return Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(
                format!("  {}", rest),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
        return Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(
                rest.to_string(),
                Style::default()
                    .fg(theme.text_strong)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    // Bullet lists
    if let Some(rest) = trimmed.strip_prefix("- ") {
        let mut spans = vec![
            Span::raw(indent.to_string()),
            Span::styled("  • ", Style::default().fg(theme.text_muted)),
        ];
        spans.extend(parse_inline_spans(rest, theme));
        return Line::from(spans);
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        let mut spans = vec![
            Span::raw(indent.to_string()),
            Span::styled("  • ", Style::default().fg(theme.text_muted)),
        ];
        spans.extend(parse_inline_spans(rest, theme));
        return Line::from(spans);
    }

    // Regular line with inline formatting
    let mut spans = vec![Span::raw(indent.to_string())];
    spans.extend(parse_inline_spans(trimmed, theme));
    Line::from(spans)
}

/// Parse inline Markdown spans: `code`, **bold**, *italic*
fn parse_inline_spans<'a>(text: &str, theme: &TuiTheme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the next special marker
        let next_backtick = remaining.find('`');
        let next_double_star = remaining.find("**");
        let next_star = remaining.find('*');

        // Find the earliest marker
        let earliest = [
            next_backtick.map(|p| (p, '`')),
            next_double_star.map(|p| (p, 'B')), // B = bold **
            next_star
                .filter(|&p| next_double_star != Some(p))
                .map(|p| (p, '*')),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(pos, _)| *pos);

        match earliest {
            None => {
                // No more markers, push the rest
                spans.push(Span::raw(remaining.to_string()));
                break;
            }
            Some((pos, '`')) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let after = &remaining[pos + 1..];
                if let Some(end) = after.find('`') {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default()
                            .fg(theme.primary)
                            .bg(Color::Rgb(40, 40, 40)),
                    ));
                    remaining = &after[end + 1..];
                } else {
                    spans.push(Span::raw(remaining[pos..].to_string()));
                    break;
                }
            }
            Some((pos, 'B')) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let after = &remaining[pos + 2..];
                if let Some(end) = after.find("**") {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default()
                            .fg(theme.text_strong)
                            .add_modifier(Modifier::BOLD),
                    ));
                    remaining = &after[end + 2..];
                } else {
                    spans.push(Span::raw(remaining[pos..].to_string()));
                    break;
                }
            }
            Some((pos, '*')) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let after = &remaining[pos + 1..];
                if let Some(end) = after.find('*') {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default().add_modifier(Modifier::ITALIC),
                    ));
                    remaining = &after[end + 1..];
                } else {
                    spans.push(Span::raw(remaining[pos..].to_string()));
                    break;
                }
            }
            _ => {
                spans.push(Span::raw(remaining.to_string()));
                break;
            }
        }
    }
    spans
}

async fn run_repl_tui(
    _config: &ReplConfig,
    viz_state: &mut ReplVisualizationState,
    agent_manager: Arc<AgentModeManager>,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut entries: Vec<ChatEntry> = vec![ChatEntry::SystemNote(
        "NDC — describe what you want, press Enter.  /help for commands".to_string(),
    )];
    let keymap = ReplTuiKeymap::from_env();

    let mut input = String::new();
    let mut input_history = InputHistory::new(100);
    let mut completion_state: Option<ReplCommandCompletionState> = None;
    let mut processing_handle: Option<
        tokio::task::JoinHandle<Result<ndc_core::AgentResponse, ndc_core::AgentError>>,
    > = None;
    let mut streamed_count = 0usize;
    let mut streamed_any = false;
    let mut last_poll = Instant::now();
    let mut should_quit = false;
    let mut session_view = TuiSessionViewState::default();
    let mut turn_counter: usize = 0;
    let mut live_events: Option<
        tokio::sync::broadcast::Receiver<ndc_core::AgentSessionExecutionEvent>,
    > = None;
    let mut live_session_id: Option<String> = None;

    while !should_quit {
        if viz_state.live_events_enabled
            && drain_live_chat_entries(
                &mut live_events,
                live_session_id.as_deref(),
                viz_state,
                &mut entries,
            )
        {
            streamed_any = true;
        }

        let status = agent_manager.status().await;
        let is_processing = processing_handle.is_some();
        let stream_state = resolve_stream_state(
            is_processing,
            viz_state.live_events_enabled,
            live_events.is_some(),
        );

        terminal.draw(|f| {
            let theme = TuiTheme::default_dark();
            let has_permission = viz_state.permission_blocked;
            let il = input_line_count(&input);
            let constraints = tui_layout_constraints(has_permission, il);
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(f.area());

            let n = areas.len();
            let body_idx = 2;
            let hint_idx = n - 2;
            let input_idx = n - 1;

            // [0] Title bar
            let title_bar = build_title_bar(&status, is_processing, None, &theme);
            f.render_widget(
                Paragraph::new(title_bar).style(Style::default().bg(Color::Rgb(30, 30, 30))),
                areas[0],
            );

            // [1] Workflow progress bar
            let progress = build_workflow_progress_bar(&viz_state, &theme);
            f.render_widget(Paragraph::new(progress), areas[1]);

            // [2] Conversation body
            let body_block = Block::default()
                .title(Span::styled(
                    " Conversation ",
                    Style::default().fg(theme.primary),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_normal));
            let inner = body_block.inner(areas[body_idx]);
            session_view.body_height = (inner.height as usize).max(1);
            let styled_lines = style_chat_entries(entries.as_slice());
            let display_line_count = styled_lines.len();
            let scroll = effective_chat_scroll(&entries, &session_view) as u16;
            let body = Paragraph::new(Text::from(styled_lines))
                .block(body_block)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0));
            f.render_widget(body, areas[body_idx]);
            if display_line_count > session_view.body_height {
                let mut scrollbar_state = ScrollbarState::new(display_line_count)
                    .position(effective_chat_scroll(&entries, &session_view));
                let scrollbar = Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight)
                    .thumb_style(Style::default().fg(theme.text_muted));
                f.render_stateful_widget(scrollbar, areas[body_idx], &mut scrollbar_state);
            }

            // [3] Permission bar (conditional)
            if has_permission {
                let perm_lines = build_permission_bar(&viz_state, &theme);
                f.render_widget(Paragraph::new(Text::from(perm_lines)), areas[3]);
            }

            // [n-2] Status / hint bar
            let hint_line = build_status_hint_bar(
                input.as_str(),
                completion_state.as_ref(),
                &viz_state,
                stream_state,
                &theme,
            );
            f.render_widget(
                Paragraph::new(hint_line).style(Style::default().bg(Color::Rgb(25, 25, 25))),
                areas[hint_idx],
            );

            // [n-1] Input area (multiline)
            let multiline_hint = if input.contains('\n') {
                " (multiline) "
            } else {
                ""
            };
            let input_title_text = format!(" > {}", multiline_hint);
            let input_block = Block::default()
                .title(Span::styled(
                    input_title_text,
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_active));
            let input_lines: Vec<Line<'_>> = input
                .split('\n')
                .map(|l| Line::from(l.to_string()))
                .collect();
            let input_widget = Paragraph::new(Text::from(input_lines)).block(input_block);
            f.render_widget(input_widget, areas[input_idx]);
            // Cursor at end of last line
            let last_line = input.rsplit('\n').next().unwrap_or(&input);
            let cursor_line_offset = input.chars().filter(|c| *c == '\n').count() as u16;
            let x = areas[input_idx].x + 1 + last_line.len() as u16;
            let y = areas[input_idx].y + 1 + cursor_line_offset;
            f.set_cursor_position((x, y));
        })?;

        if let Some(handle) = processing_handle.as_ref() {
            if live_events.is_none() && last_poll.elapsed() >= Duration::from_millis(120) {
                if let Ok(events) = agent_manager
                    .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
                    .await
                    && events.len() > streamed_count
                {
                    let new_events = &events[streamed_count..];
                    append_timeline_events(
                        &mut viz_state.timeline_cache,
                        new_events,
                        TIMELINE_CACHE_MAX_EVENTS,
                    );
                    for event in new_events {
                        push_chat_entries(&mut entries, event_to_entries(event, viz_state));
                    }
                    streamed_count = events.len();
                    streamed_any = true;
                }
                last_poll = Instant::now();
            }

            if handle.is_finished() {
                let handle = processing_handle.take().expect("present");
                match handle.await {
                    Ok(Ok(response)) => {
                        if !streamed_any {
                            append_timeline_events(
                                &mut viz_state.timeline_cache,
                                &response.execution_events,
                                TIMELINE_CACHE_MAX_EVENTS,
                            );
                            for event in &response.execution_events {
                                push_chat_entries(&mut entries, event_to_entries(event, viz_state));
                            }
                        }
                        if !response.content.trim().is_empty() {
                            push_chat_entry(&mut entries, ChatEntry::Separator);
                            push_chat_entry(
                                &mut entries,
                                ChatEntry::AssistantMessage {
                                    content: response.content.clone(),
                                    turn_id: turn_counter,
                                },
                            );
                        }
                    }
                    Ok(Err(e)) => {
                        push_chat_entry(
                            &mut entries,
                            ChatEntry::ErrorNote(format!("[Error] {}", e)),
                        );
                    }
                    Err(e) => {
                        push_chat_entry(
                            &mut entries,
                            ChatEntry::ErrorNote(format!("[Error] join failed: {}", e)),
                        );
                    }
                }
            }
        }

        if event::poll(Duration::from_millis(20))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if key.code == KeyCode::Esc
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL))
                    {
                        should_quit = true;
                        continue;
                    }

                    if handle_session_scroll_key(
                        &key,
                        &mut session_view,
                        total_display_lines(&entries),
                    ) {
                        continue;
                    }

                    if let Some(action) = detect_tui_shortcut(&key, &keymap) {
                        apply_tui_shortcut_action(action, viz_state, entries.as_mut());
                        continue;
                    }

                    if processing_handle.is_some() {
                        continue;
                    }

                    match key.code {
                        KeyCode::Tab => {
                            if apply_slash_completion(&mut input, &mut completion_state, false) {
                                continue;
                            }
                        }
                        KeyCode::BackTab => {
                            if apply_slash_completion(&mut input, &mut completion_state, true) {
                                continue;
                            }
                        }
                        KeyCode::Up => {
                            if let Some(prev) = input_history.up(&input) {
                                input = prev.to_string();
                            }
                            continue;
                        }
                        KeyCode::Down => {
                            if let Some(next) = input_history.down() {
                                input = next.to_string();
                            }
                            continue;
                        }
                        KeyCode::Enter
                            if key.modifiers.contains(KeyModifiers::SHIFT)
                                || key.modifiers.contains(KeyModifiers::ALT) =>
                        {
                            input.push('\n');
                            continue;
                        }
                        KeyCode::Enter => {
                            let cmd = input.trim().to_string();
                            input.clear();
                            completion_state = None;
                            input_history.push(cmd.clone());
                            input_history.reset();
                            if cmd.is_empty() {
                                continue;
                            }
                            if cmd == "exit" || cmd == "quit" || cmd == "q" {
                                should_quit = true;
                                continue;
                            }
                            if cmd.starts_with('/') {
                                if handle_tui_command(
                                    &cmd,
                                    viz_state,
                                    agent_manager.clone(),
                                    &mut entries,
                                )
                                .await?
                                {
                                    should_quit = true;
                                }
                                continue;
                            }

                            turn_counter += 1;
                            push_chat_entry(&mut entries, ChatEntry::Separator);
                            push_chat_entry(
                                &mut entries,
                                ChatEntry::UserMessage {
                                    content: cmd.clone(),
                                    turn_id: turn_counter,
                                },
                            );
                            push_chat_entry(
                                &mut entries,
                                ChatEntry::SystemNote("processing...".to_string()),
                            );
                            session_view.auto_follow = true;
                            live_events = None;
                            live_session_id = None;

                            streamed_count = agent_manager
                                .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
                                .await
                                .map(|events| events.len())
                                .unwrap_or(0);
                            streamed_any = false;
                            last_poll = Instant::now();
                            if viz_state.live_events_enabled {
                                match agent_manager.subscribe_execution_events().await {
                                    Ok((session_id, rx)) => {
                                        live_session_id = Some(session_id);
                                        live_events = Some(rx);
                                    }
                                    Err(e) => {
                                        push_chat_entry(
                                            &mut entries,
                                            ChatEntry::WarningNote(format!(
                                                "[Warning] realtime stream unavailable: {}",
                                                e
                                            )),
                                        );
                                    }
                                }
                            } else {
                                push_chat_entry(
                                    &mut entries,
                                    ChatEntry::SystemNote(
                                        "[Tip] realtime stream is OFF, using polling fallback"
                                            .to_string(),
                                    ),
                                );
                            }
                            let manager = agent_manager.clone();
                            processing_handle =
                                Some(tokio::spawn(
                                    async move { manager.process_input(&cmd).await },
                                ));
                        }
                        KeyCode::Backspace => {
                            input.pop();
                            completion_state = None;
                        }
                        KeyCode::Char(ch) => {
                            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                                input.push(ch);
                                completion_state = None;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    let _ = handle_session_scroll_mouse(
                        &mouse,
                        &mut session_view,
                        total_display_lines(&entries),
                    );
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// ===== 命令处理 (极简版) =====

async fn handle_command(
    input: &str,
    _config: &ReplConfig,
    viz_state: &mut ReplVisualizationState,
    agent_manager: Arc<AgentModeManager>,
) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts[0];

    match cmd {
        "/help" | "/h" => show_help(),
        "/provider" | "/providers" | "/p" => {
            if parts.len() > 1 {
                let provider = parts[1];
                if let Err(e) = agent_manager.switch_provider(provider, None).await {
                    println!("[Error] Failed to switch provider: {}", e);
                    return;
                }
                let status = agent_manager.status().await;
                println!(
                    "[OK] Provider switched to '{}' with model '{}'",
                    status.provider, status.model
                );
            } else {
                let status = agent_manager.status().await;
                println!("Current provider: {}", status.provider);
                println!("Available providers: {}", AVAILABLE_PROVIDERS.join(", "));
                println!("Usage: /provider <name>");
            }
        }
        "/model" | "/m" => {
            if parts.len() > 1 {
                let model_spec = parts[1];
                if let Some(idx) = model_spec.find('/') {
                    // backward compatibility: /model provider/model
                    let provider = &model_spec[..idx];
                    let model = &model_spec[idx + 1..];
                    if let Err(e) = agent_manager.switch_provider(provider, Some(model)).await {
                        println!("[Error] Failed to switch provider/model: {}", e);
                        return;
                    }
                    let status = agent_manager.status().await;
                    println!(
                        "[OK] Provider '{}' using model '{}'",
                        status.provider, status.model
                    );
                } else {
                    if let Err(e) = agent_manager.switch_model(model_spec).await {
                        println!("[Error] Failed to switch model: {}", e);
                        return;
                    }
                    let status = agent_manager.status().await;
                    println!(
                        "[OK] Model switched to '{}' (provider: {})",
                        status.model, status.provider
                    );
                }
            } else {
                show_model_info(agent_manager.as_ref()).await;
            }
        }
        "/status" | "/st" => {
            let status = agent_manager.status().await;
            show_agent_status(&status);
        }
        "/agent" => {
            // OpenCode 风格: /agent 命令用于管理 agent 而非切换模式
            let agent_input = if parts.len() > 1 {
                format!("/agent {}", &input[7..])
            } else {
                "/agent help".to_string()
            };
            if let Err(e) = handle_agent_command(&agent_input, &agent_manager).await {
                println!("[Error] {}", e);
            }
        }
        "/clear" | "/cls" => {
            print!("\x1B[2J\x1B[3J\x1B[H");
            let _ = io::stdout().flush();
        }
        "/thinking" | "/t" => {
            if parts.len() > 1 && (parts[1] == "show" || parts[1] == "now") {
                show_recent_thinking(
                    viz_state.timeline_cache.as_slice(),
                    viz_state.timeline_limit,
                    viz_state.redaction_mode,
                );
                return;
            }
            viz_state.show_thinking = !viz_state.show_thinking;
            if viz_state.show_thinking {
                viz_state.hidden_thinking_round_hints.clear();
            }
            println!(
                "[OK] Thinking display: {}",
                if viz_state.show_thinking {
                    "ON"
                } else {
                    "OFF (collapsed)"
                }
            );
        }
        "/details" | "/d" => {
            viz_state.show_tool_details = !viz_state.show_tool_details;
            println!(
                "[OK] Tool details: {}",
                if viz_state.show_tool_details {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }
        "/cards" | "/toolcards" => {
            viz_state.expand_tool_cards = !viz_state.expand_tool_cards;
            println!(
                "[OK] Tool cards: {}",
                if viz_state.expand_tool_cards {
                    "EXPANDED"
                } else {
                    "COLLAPSED"
                }
            );
        }
        "/verbosity" | "/v" => {
            if parts.len() > 1 {
                if let Some(v) = DisplayVerbosity::parse(parts[1]) {
                    viz_state.verbosity = v;
                    println!("[OK] Verbosity: {}", v.label());
                } else {
                    println!(
                        "[Error] Unknown verbosity level '{}'. Use: compact, normal, verbose",
                        parts[1]
                    );
                }
            } else {
                viz_state.verbosity = viz_state.verbosity.next();
                println!("[OK] Verbosity: {}", viz_state.verbosity.label());
            }
        }
        "/timeline" => {
            if parts.len() > 1 {
                if let Ok(parsed) = parts[1].parse::<usize>() {
                    viz_state.timeline_limit = parsed.max(1);
                } else {
                    println!("[Error] timeline limit must be a positive integer");
                    return;
                }
            }
            match agent_manager
                .session_timeline(Some(viz_state.timeline_limit))
                .await
            {
                Ok(events) => {
                    viz_state.timeline_cache = events;
                    show_timeline(
                        viz_state.timeline_cache.as_slice(),
                        viz_state.timeline_limit,
                        viz_state.redaction_mode,
                    );
                }
                Err(e) => {
                    println!("[Warning] Failed to fetch session timeline: {}", e);
                    show_timeline(
                        viz_state.timeline_cache.as_slice(),
                        viz_state.timeline_limit,
                        viz_state.redaction_mode,
                    );
                }
            }
        }
        "/stream" => match apply_stream_command(viz_state, parts.get(1).copied()) {
            Ok(message) => println!("[OK] {}", message),
            Err(message) => println!("[Error] {}", message),
        },
        "/workflow" => {
            let mode = match WorkflowOverviewMode::parse(parts.get(1).copied()) {
                Ok(mode) => mode,
                Err(message) => {
                    println!("[Error] {}", message);
                    return;
                }
            };
            show_workflow_overview(
                viz_state.timeline_cache.as_slice(),
                viz_state.timeline_limit,
                viz_state.redaction_mode,
                viz_state.current_workflow_stage.as_deref(),
                viz_state.current_workflow_stage_index,
                viz_state.current_workflow_stage_total,
                mode,
            );
        }
        "/tokens" => match apply_tokens_command(viz_state, parts.get(1).copied()) {
            Ok(message) => println!("[OK] {}", message),
            Err(message) => println!("[Error] {}", message),
        },
        "/metrics" => {
            show_runtime_metrics(viz_state);
        }
        _ => {
            // 未知命令，尝试作为自然语言处理
            println!("[Tip] Unknown command. Use natural language or type '/help' for commands.");
        }
    }
}

// ===== Agent 对话处理 =====

/// 处理用户输入 (OpenCode 风格: 直接发送给 AI)
async fn handle_agent_dialogue(
    input: &str,
    agent_manager: &Arc<AgentModeManager>,
    viz_state: &mut ReplVisualizationState,
) {
    println!("[Agent] processing...");
    let input_owned = input.to_string();
    let manager = agent_manager.clone();
    let handle = tokio::spawn(async move { manager.process_input(&input_owned).await });

    let mut streamed_count = agent_manager
        .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
        .await
        .map(|events| events.len())
        .unwrap_or(0);
    let mut streamed_any = false;

    loop {
        if handle.is_finished() {
            break;
        }

        if let Ok(events) = agent_manager
            .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
            .await
            && events.len() > streamed_count
        {
            let new_events = &events[streamed_count..];
            append_timeline_events(
                &mut viz_state.timeline_cache,
                new_events,
                TIMELINE_CACHE_MAX_EVENTS,
            );
            render_execution_events(new_events, viz_state);
            streamed_count = events.len();
            streamed_any = true;
        }

        tokio::time::sleep(Duration::from_millis(120)).await;
    }

    if let Ok(events) = agent_manager
        .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
        .await
        && events.len() > streamed_count
    {
        let new_events = &events[streamed_count..];
        append_timeline_events(
            &mut viz_state.timeline_cache,
            new_events,
            TIMELINE_CACHE_MAX_EVENTS,
        );
        render_execution_events(new_events, viz_state);
        streamed_any = true;
    }

    match handle.await {
        Ok(Ok(response)) => {
            if !streamed_any {
                append_timeline_events(
                    &mut viz_state.timeline_cache,
                    &response.execution_events,
                    TIMELINE_CACHE_MAX_EVENTS,
                );
                render_execution_events(&response.execution_events, viz_state);
            }
            if let Ok(events) = agent_manager
                .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
                .await
            {
                viz_state.timeline_cache = events;
            }

            if !response.content.is_empty() {
                println!();
                println!("{}", response.content);
            }

            if !response.tool_calls.is_empty() {
                let tool_names: Vec<&str> = response
                    .tool_calls
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect();
                println!("\n[Tools: {}]", tool_names.join(", "));
            }

            if let Some(verification) = response.verification_result {
                match verification {
                    ndc_core::VerificationResult::Completed => {
                        println!("[OK] Verification passed!");
                    }
                    ndc_core::VerificationResult::Incomplete { reason } => {
                        println!("[Incomplete] {}", reason);
                    }
                    ndc_core::VerificationResult::QualityGateFailed { reason } => {
                        println!("[Failed] Quality gate: {}", reason);
                    }
                }
            }

            println!();
        }
        Ok(Err(e)) => {
            show_agent_error(&e, agent_manager).await;
        }
        Err(e) => {
            println!("[Error] agent task join failed: {}", e);
        }
    }
}

/// 显示详细的 Agent 错误信息
async fn show_agent_error(error: &ndc_core::AgentError, agent_manager: &Arc<AgentModeManager>) {
    let status = agent_manager.status().await;
    let error_msg = error.to_string();

    let display_error = if error_msg.len() > 72 {
        &error_msg[..72]
    } else {
        &error_msg
    };

    println!();
    println!("+--------------------------------------------------------------------+");
    println!("|  Agent Error                                                        |");
    println!("+--------------------------------------------------------------------+");
    println!("|  {}                 ", display_error);
    println!("+--------------------------------------------------------------------+");
    println!("|  Full Error:                                                       |");
    println!("|    {} ", error_msg);
    println!("+--------------------------------------------------------------------+");
    println!("|  Provider Configuration:                                            |");
    println!(
        "|    Provider: {}                                                    ",
        status.provider
    );
    println!(
        "|    Model: {}                                                       ",
        status.model
    );
    println!("+--------------------------------------------------------------------+");
    println!("|  Configuration Sources Checked:                                     |");

    // 检查各配置来源
    let provider_upper = status.provider.to_uppercase();

    // 环境变量
    let api_key_env = format!("NDC_{}_API_KEY", provider_upper);
    let group_id_env = format!("NDC_{}_GROUP_ID", provider_upper);

    let api_key_set = std::env::var(&api_key_env).is_ok();
    let group_id_set = std::env::var(&group_id_env).is_ok();

    println!(
        "|    Env: {}={}                 ",
        api_key_env,
        if api_key_set { "[SET]" } else { "[NOT SET]" }
    );
    println!(
        "|    Env: {}={}      ",
        group_id_env,
        if group_id_set { "[SET]" } else { "[NOT SET]" }
    );

    // 检查配置文件
    let config_paths = [
        ("Project", ".ndc/config.yaml"),
        ("User", "~/.config/ndc/config.yaml"),
        ("Global", "/etc/ndc/config.yaml"),
    ];

    for (name, path) in &config_paths {
        let expanded = if path.starts_with("~") {
            if let Ok(home) = std::env::var("HOME") {
                path.replace("~", &home)
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        };

        let exists = std::path::Path::new(&expanded).exists();
        println!(
            "|    {} Config: [{}] {}                 ",
            name,
            if exists { "FOUND" } else { "NOT FOUND" },
            path
        );
    }

    println!("+--------------------------------------------------------------------+");
    println!("|  How to Fix:                                                        |");
    println!(
        "|    1. Set API key: export NDC_{}_API_KEY=\"your-key\"           ",
        provider_upper
    );
    if provider_upper == "MINIMAX" {
        println!("|    2. (Optional) Set model: export NDC_MINIMAX_MODEL=\"MiniMax-M2.5\" ");
    }
    println!("|    3. Restart REPL or try: /provider openai                       ");
    println!("+--------------------------------------------------------------------+");
    println!();
}

// ===== 辅助函数 =====

fn show_help() {
    println!(
        r#"
Available Commands:
  /help, /h       Show this help
  /provider[s], /p Switch provider (e.g., /provider minimax)
  /model, /m      List current provider models or switch model
  /agent          Manage agent settings
  /status         Show agent status
  /thinking, /t   Toggle reasoning display (default collapsed)
  /thinking show  Show recent thinking immediately
  /details, /d    Toggle tool step/details display
  /cards          Toggle tool cards expanded/collapsed
  /verbosity, /v  Cycle display verbosity (compact/normal/verbose)
  /stream [mode]  Toggle realtime event stream (on/off/status)
  /workflow [mode] Show workflow overview (compact|verbose; default verbose)
  /tokens [mode]  Token metrics: show/hide/reset/status
  /metrics        Runtime metrics (tools/errors/permission/tokens)
  /timeline [N]   Show recent execution timeline (default N=40)
  /clear          Clear screen
  exit, quit, q   Exit REPL

TUI Shortcuts:
  Ctrl+T          Toggle thinking
  Ctrl+D          Cycle verbosity (compact→normal→verbose)
  Ctrl+E          Toggle tool cards
  Ctrl+Y          Show recent thinking
  Ctrl+I          Show recent timeline
  Ctrl+L          Clear session panel
  Up/Down         Scroll session by line
  PgUp/PgDn       Scroll session by half page
  Home/End        Jump to top/bottom
  Mouse Wheel     Scroll session
  Tab/Shift+Tab   Complete command/argument (see Hints panel)

Natural Language Examples:
  "Create a REST API for user management"
  "Fix the bug in authentication"
  "Run tests for the payment module"
  "Explain how the system works"

LLM Providers: minimax, minimax-coding-plan, minimax-cn, minimax-cn-coding-plan, openrouter, openai, anthropic, ollama

Environment Variables:
  NDC_MINIMAX_API_KEY, NDC_OPENAI_API_KEY, etc.
  NDC_TOOL_CARDS_EXPANDED=true|false
  NDC_REPL_KEY_TOGGLE_THINKING=t
  NDC_REPL_KEY_TOGGLE_DETAILS=d
  NDC_REPL_KEY_TOGGLE_TOOL_CARDS=e
  NDC_REPL_KEY_SHOW_RECENT_THINKING=y
  NDC_REPL_KEY_SHOW_TIMELINE=i
  NDC_REPL_KEY_CLEAR_PANEL=l
  NDC_REPL_LIVE_EVENTS=true|false
  NDC_REPL_SHOW_USAGE=true|false
  Provider options: openai, anthropic, minimax, minimax-coding-plan, minimax-cn, minimax-cn-coding-plan, openrouter, ollama
"#
    );
}

fn append_timeline_events(
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

fn push_log_line(logs: &mut Vec<String>, line: &str) {
    if line.contains('\n') {
        for part in line.lines() {
            logs.push(part.to_string());
        }
    } else {
        logs.push(line.to_string());
    }
    if logs.len() > TUI_MAX_LOG_LINES {
        let overflow = logs.len() - TUI_MAX_LOG_LINES;
        logs.drain(0..overflow);
    }
}

fn drain_live_execution_events(
    receiver: &mut Option<tokio::sync::broadcast::Receiver<ndc_core::AgentSessionExecutionEvent>>,
    expected_session_id: Option<&str>,
    viz_state: &mut ReplVisualizationState,
    logs: &mut Vec<String>,
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
                for line in event_to_lines(&message.event, viz_state) {
                    push_log_line(logs, &line);
                }
                rendered = true;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                push_log_line(
                    logs,
                    &format!(
                        "[Warning] realtime stream lagged, dropped {} event(s)",
                        skipped
                    ),
                );
                rendered = true;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                *receiver = None;
                push_log_line(
                    logs,
                    "[Warning] realtime stream closed, fallback to polling",
                );
                rendered = true;
                break;
            }
        }
    }
    rendered
}

fn event_to_lines(
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
                        // Single line: ◆ Planning...
                        lines.push(format!("[Stage] {}...", capitalize_stage(stage.as_str())));
                    }
                    DisplayVerbosity::Normal => {
                        // Stage + detail
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
                        // Original two-line format
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
                    // Single line with human-readable summary
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
            let duration = event.duration_ms.map(|d| format_duration_ms(d));
            let status_icon = if event.is_error { "✗" } else { "✓" };

            match v {
                DisplayVerbosity::Compact => {
                    // Single line: ✓ shell (1.2s) or ✗ shell — error message
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
                    } else {
                        if let Some(preview) = extract_tool_result_preview(&event.message) {
                            let (msg, truncated) = truncate_output(
                                &sanitize_text(preview, viz_state.redaction_mode),
                                100,
                            );
                            lines.push(format!("[ToolEnd] {} {}{}", status_icon, tool, dur));
                            let suffix = if truncated { " …" } else { "" };
                            lines.push(format!("  └─ {}{}", msg, suffix));
                        } else {
                            lines.push(format!("[ToolEnd] {} {}{}", status_icon, tool, dur));
                        }
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
                    if viz_state.expand_tool_cards {
                        if let Some(args) = extract_tool_args_preview(&event.message) {
                            lines.push(format!(
                                "  └─ input : {}",
                                sanitize_text(args, viz_state.redaction_mode)
                            ));
                        }
                    }
                }
                DisplayVerbosity::Verbose => {
                    // Original full format
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
                DisplayVerbosity::Compact => {
                    // Hidden — already shown in status bar
                }
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
                    // Extract tool name from message if possible
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
        | ndc_core::AgentExecutionEventKind::Verification => {
            match v {
                DisplayVerbosity::Compact => {
                    // Hide steps entirely in compact mode
                }
                DisplayVerbosity::Normal => {
                    // Only show finish with duration
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
            }
        }
        ndc_core::AgentExecutionEventKind::Error => {
            lines.push(format!(
                "[Error][r{}] {}",
                event.round,
                sanitize_text(&event.message, viz_state.redaction_mode)
            ));
        }
        ndc_core::AgentExecutionEventKind::SessionStatus
        | ndc_core::AgentExecutionEventKind::Text => {}
    }
    lines
}

fn append_recent_thinking(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
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

fn append_recent_timeline(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ReplRuntimeMetrics {
    tool_calls_total: usize,
    tool_calls_failed: usize,
    tool_duration_samples: usize,
    tool_duration_total_ms: u64,
    permission_waits: usize,
    error_events: usize,
}

impl ReplRuntimeMetrics {
    fn avg_tool_duration_ms(self) -> Option<u64> {
        if self.tool_duration_samples == 0 {
            None
        } else {
            Some(self.tool_duration_total_ms / self.tool_duration_samples as u64)
        }
    }

    fn tool_error_rate_percent(self) -> u64 {
        if self.tool_calls_total == 0 {
            0
        } else {
            ((self.tool_calls_failed as u64) * 100) / (self.tool_calls_total as u64)
        }
    }
}

fn compute_runtime_metrics(timeline: &[ndc_core::AgentExecutionEvent]) -> ReplRuntimeMetrics {
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
struct WorkflowStageProgress {
    count: usize,
    total_ms: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct WorkflowProgressSummary {
    stages: std::collections::BTreeMap<String, WorkflowStageProgress>,
    current_stage: Option<String>,
    current_stage_active_ms: u64,
    history_may_be_partial: bool,
}

fn compute_workflow_progress_summary(
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

fn group_timeline_by_stage<'a>(
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

fn append_workflow_overview(
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

fn append_token_usage(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
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

fn append_runtime_metrics(entries: &mut Vec<ChatEntry>, viz_state: &ReplVisualizationState) {
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

fn apply_tui_shortcut_action(
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
    }
}

fn render_execution_events(
    events: &[ndc_core::AgentExecutionEvent],
    viz_state: &mut ReplVisualizationState,
) {
    for event in events {
        for line in event_to_lines(event, viz_state) {
            println!("{}", line);
        }
    }
}

async fn restore_session_to_panel(
    agent_manager: &Arc<AgentModeManager>,
    viz_state: &mut ReplVisualizationState,
    entries: &mut Vec<ChatEntry>,
) {
    match agent_manager
        .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
        .await
    {
        Ok(events) if !events.is_empty() => {
            viz_state.timeline_cache = events.clone();
            push_text_entry(entries, "--- Restored session history ---");
            for event in &events {
                push_chat_entries(entries, event_to_entries(event, viz_state));
            }
            push_text_entry(entries, "---");
        }
        Ok(_) => {}
        Err(e) => push_text_entry(
            entries,
            &format!("[Warning] Could not restore session history: {}", e),
        ),
    }
}

async fn handle_tui_command(
    input: &str,
    viz_state: &mut ReplVisualizationState,
    agent_manager: Arc<AgentModeManager>,
    entries: &mut Vec<ChatEntry>,
) -> io::Result<bool> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts[0] {
        "/help" | "/h" => {
            push_text_entry(
                entries,
                "Commands: /help /provider /model /status /workflow /tokens /metrics /t /d /cards /v /stream /thinking /timeline [N] /copy /resume [id] [--cross] /new /session [N] /project [dir] /clear /exit",
            );
            push_text_entry(
                entries,
                "Shortcuts: Ctrl+T / Ctrl+D / Ctrl+E / Ctrl+Y / Ctrl+I / Ctrl+L",
            );
            push_text_entry(
                entries,
                "Scroll: Up/Down line, PgUp/PgDn half-page, Home/End top-bottom, drag to select",
            );
        }
        "/thinking" | "/t" => {
            if parts.len() > 1 && (parts[1] == "show" || parts[1] == "now") {
                append_recent_thinking(entries, viz_state);
            } else {
                viz_state.show_thinking = !viz_state.show_thinking;
                if viz_state.show_thinking {
                    viz_state.hidden_thinking_round_hints.clear();
                }
                toggle_all_reasoning_blocks(entries);
                push_text_entry(
                    entries,
                    &format!(
                        "[OK] Thinking display: {}",
                        if viz_state.show_thinking {
                            "ON"
                        } else {
                            "OFF (collapsed)"
                        }
                    ),
                );
            }
        }
        "/details" | "/d" => {
            viz_state.show_tool_details = !viz_state.show_tool_details;
            push_text_entry(
                entries,
                &format!(
                    "[OK] Tool details: {}",
                    if viz_state.show_tool_details {
                        "ON"
                    } else {
                        "OFF"
                    }
                ),
            );
        }
        "/cards" | "/toolcards" => {
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
        "/verbosity" | "/v" => {
            if parts.len() > 1 {
                if let Some(v) = DisplayVerbosity::parse(parts[1]) {
                    viz_state.verbosity = v;
                    push_text_entry(entries, &format!("[OK] Verbosity: {}", v.label()));
                } else {
                    push_text_entry(
                        entries,
                        &format!(
                            "[Error] Unknown verbosity '{}'. Use: compact, normal, verbose",
                            parts[1]
                        ),
                    );
                }
            } else {
                viz_state.verbosity = viz_state.verbosity.next();
                push_text_entry(
                    entries,
                    &format!("[OK] Verbosity: {}", viz_state.verbosity.label()),
                );
            }
        }
        "/stream" => match apply_stream_command(viz_state, parts.get(1).copied()) {
            Ok(message) => push_text_entry(entries, &format!("[OK] {}", message)),
            Err(message) => push_text_entry(entries, &format!("[Error] {}", message)),
        },
        "/workflow" => {
            let mode = match WorkflowOverviewMode::parse(parts.get(1).copied()) {
                Ok(mode) => mode,
                Err(message) => {
                    push_text_entry(entries, &format!("[Error] {}", message));
                    return Ok(false);
                }
            };
            append_workflow_overview(entries, viz_state, mode);
        }
        "/tokens" => match apply_tokens_command(viz_state, parts.get(1).copied()) {
            Ok(message) => {
                push_text_entry(entries, &format!("[OK] {}", message));
                append_token_usage(entries, viz_state);
            }
            Err(message) => push_text_entry(entries, &format!("[Error] {}", message)),
        },
        "/metrics" => {
            append_runtime_metrics(entries, viz_state);
        }
        "/timeline" => {
            if parts.len() > 1
                && let Ok(parsed) = parts[1].parse::<usize>()
            {
                viz_state.timeline_limit = parsed.max(1);
            }
            match agent_manager
                .session_timeline(Some(viz_state.timeline_limit))
                .await
            {
                Ok(events) => {
                    viz_state.timeline_cache = events;
                    append_recent_timeline(entries, viz_state);
                }
                Err(e) => push_text_entry(entries, &format!("[Warning] {}", e)),
            }
        }
        "/provider" | "/providers" | "/p" => {
            if parts.len() > 1 {
                if let Err(e) = agent_manager.switch_provider(parts[1], None).await {
                    push_text_entry(entries, &format!("[Error] {}", e));
                } else {
                    let status = agent_manager.status().await;
                    push_text_entry(
                        entries,
                        &format!(
                            "[OK] Provider switched to '{}' with model '{}'",
                            status.provider, status.model
                        ),
                    );
                }
            } else {
                let status = agent_manager.status().await;
                push_text_entry(entries, &format!("Current provider: {}", status.provider));
                push_text_entry(
                    entries,
                    &format!("Available providers: {}", AVAILABLE_PROVIDERS.join(", ")),
                );
                push_text_entry(entries, "Usage: /provider <name>");
            }
        }
        "/model" | "/m" => {
            if parts.len() > 1 {
                if let Some(idx) = parts[1].find('/') {
                    let provider = &parts[1][..idx];
                    let model = &parts[1][idx + 1..];
                    if let Err(e) = agent_manager.switch_provider(provider, Some(model)).await {
                        push_text_entry(entries, &format!("[Error] {}", e));
                    } else {
                        let status = agent_manager.status().await;
                        push_text_entry(
                            entries,
                            &format!(
                                "[OK] Provider '{}' using model '{}'",
                                status.provider, status.model
                            ),
                        );
                    }
                } else if let Err(e) = agent_manager.switch_model(parts[1]).await {
                    push_text_entry(entries, &format!("[Error] {}", e));
                }
            } else {
                let status = agent_manager.status().await;
                push_text_entry(entries, &format!("Current model: {}", status.model));
            }
        }
        "/status" | "/st" => {
            let status = agent_manager.status().await;
            push_text_entry(
                entries,
                &format!(
                    "Agent={} Provider={} Model={} Session={}",
                    status.agent_name,
                    status.provider,
                    status.model,
                    status.session_id.unwrap_or_else(|| "-".to_string())
                ),
            );
        }
        "/agent" => {
            let agent_input = if parts.len() > 1 {
                format!("/agent {}", &input[7..])
            } else {
                "/agent help".to_string()
            };
            if let Err(e) = handle_agent_command(&agent_input, &agent_manager).await {
                push_text_entry(entries, &format!("[Error] {}", e));
            } else {
                push_text_entry(entries, "[OK] agent command executed");
            }
        }
        "/clear" | "/cls" => {
            entries.clear();
        }
        "/copy" => {
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let path = format!("/tmp/ndc-session-{}.txt", timestamp);
            match std::fs::write(&path, entries_to_plain_text(entries)) {
                Ok(()) => push_text_entry(entries, &format!("[OK] Session saved to: {}", path)),
                Err(e) => {
                    push_text_entry(entries, &format!("[Error] Failed to save session: {}", e))
                }
            }
        }
        "/resume" | "/r" => {
            let has_cross = parts.iter().any(|p| *p == "--cross");
            let session_id = parts.iter().skip(1).find(|p| !p.starts_with("--")).copied();
            let result = if let Some(sid) = session_id {
                agent_manager.use_session(sid, has_cross).await
            } else {
                agent_manager.resume_latest_project_session().await
            };
            match result {
                Ok(sid) => {
                    push_text_entry(entries, &format!("[OK] Session resumed: {}", sid));
                    restore_session_to_panel(&agent_manager, viz_state, entries).await;
                }
                Err(e) => push_text_entry(entries, &format!("[Error] {}", e)),
            }
        }
        "/new" => match agent_manager.start_new_session().await {
            Ok(sid) => {
                entries.clear();
                push_text_entry(entries, &format!("[OK] New session started: {}", sid));
            }
            Err(e) => push_text_entry(entries, &format!("[Error] {}", e)),
        },
        "/session" | "/sessions" => {
            let limit = parts
                .get(1)
                .and_then(|p| p.parse::<usize>().ok())
                .unwrap_or(10)
                .max(1);
            match agent_manager.list_project_session_ids(None, limit).await {
                Ok(ids) if ids.is_empty() => {
                    push_text_entry(entries, "[Info] No sessions for current project.");
                }
                Ok(ids) => {
                    push_text_entry(entries, "Sessions (newest first):");
                    for id in &ids {
                        push_text_entry(entries, &format!("  {}", id));
                    }
                    push_text_entry(
                        entries,
                        "Use /resume <id> to restore, or /resume for latest.",
                    );
                }
                Err(e) => push_text_entry(entries, &format!("[Error] {}", e)),
            }
        }
        "/project" | "/projects" => {
            if parts.len() > 1 {
                let dir = std::path::PathBuf::from(parts[1]);
                match agent_manager.switch_project_context(dir).await {
                    Ok(outcome) => {
                        entries.clear();
                        push_text_entry(
                            entries,
                            &format!(
                                "[OK] Switched to project '{}' ({})",
                                outcome.project_id,
                                outcome.project_root.display()
                            ),
                        );
                        push_text_entry(
                            entries,
                            &format!(
                                "Session: {} ({})",
                                outcome.session_id,
                                if outcome.resumed_existing_session {
                                    "resumed"
                                } else {
                                    "new"
                                }
                            ),
                        );
                        if outcome.resumed_existing_session {
                            push_text_entry(
                                entries,
                                "Use /resume to restore session history into this panel.",
                            );
                        }
                    }
                    Err(e) => push_text_entry(entries, &format!("[Error] {}", e)),
                }
            } else {
                match agent_manager.discover_projects(10).await {
                    Ok(candidates) if candidates.is_empty() => {
                        push_text_entry(entries, "[Info] No projects discovered.");
                    }
                    Ok(candidates) => {
                        push_text_entry(entries, "Known projects:");
                        for c in &candidates {
                            push_text_entry(
                                entries,
                                &format!("  {} — {}", c.project_id, c.project_root.display()),
                            );
                        }
                        push_text_entry(entries, "Use /project <dir> to switch.");
                    }
                    Err(e) => push_text_entry(entries, &format!("[Error] {}", e)),
                }
            }
        }
        "/exit" => return Ok(true),
        _ => push_text_entry(entries, "[Tip] Unknown command. Use /help."),
    }
    Ok(false)
}

fn show_recent_thinking(
    timeline: &[ndc_core::AgentExecutionEvent],
    limit: usize,
    mode: RedactionMode,
) {
    println!();
    println!("Recent Thinking (last {} events):", limit);
    let total = timeline.len();
    let start = total.saturating_sub(limit);
    let mut count = 0usize;
    for event in timeline.iter().skip(start) {
        if !matches!(event.kind, ndc_core::AgentExecutionEventKind::Reasoning) {
            continue;
        }
        println!(
            "  - r{} {} | {}",
            event.round,
            event.timestamp.format("%H:%M:%S"),
            sanitize_text(&event.message, mode)
        );
        count += 1;
    }
    if count == 0 {
        println!("  (no thinking events yet)");
    }
    println!();
}

fn show_workflow_overview(
    timeline: &[ndc_core::AgentExecutionEvent],
    limit: usize,
    mode: RedactionMode,
    current_stage: Option<&str>,
    current_stage_index: Option<u32>,
    current_stage_total: Option<u32>,
    overview_mode: WorkflowOverviewMode,
) {
    println!();
    println!(
        "Workflow Overview ({}): current={} progress={}",
        overview_mode.as_str(),
        current_stage.unwrap_or("-"),
        workflow_progress_descriptor(current_stage, current_stage_index, current_stage_total)
    );
    let summary = compute_workflow_progress_summary(timeline, chrono::Utc::now());
    if summary.history_may_be_partial {
        println!(
            "[Warning] workflow history may be partial (cache cap={} events)",
            TIMELINE_CACHE_MAX_EVENTS
        );
    }
    println!("Workflow Progress:");
    for stage in WORKFLOW_STAGE_ORDER {
        let metrics = summary.stages.get(*stage).copied().unwrap_or_default();
        let active_ms = if summary.current_stage.as_deref() == Some(*stage) {
            summary.current_stage_active_ms
        } else {
            0
        };
        println!(
            "  - {} count={} total_ms={} active_ms={}",
            stage, metrics.count, metrics.total_ms, active_ms
        );
    }
    if overview_mode == WorkflowOverviewMode::Verbose {
        let total = timeline.len();
        let start = total.saturating_sub(limit);
        let mut count = 0usize;
        for event in timeline.iter().skip(start) {
            if !matches!(event.kind, ndc_core::AgentExecutionEventKind::WorkflowStage) {
                continue;
            }
            println!(
                "  - r{} {} | {}",
                event.round,
                event.timestamp.format("%H:%M:%S"),
                sanitize_text(&event.message, mode)
            );
            count += 1;
        }
        if count == 0 {
            println!("  (no workflow stage events yet)");
        }
    } else {
        println!("  (use /workflow verbose to inspect stage event timeline)");
    }
    println!();
}

fn show_runtime_metrics(viz_state: &ReplVisualizationState) {
    let metrics = compute_runtime_metrics(viz_state.timeline_cache.as_slice());
    println!();
    println!("Runtime Metrics:");
    println!(
        "  - workflow_current={}",
        viz_state.current_workflow_stage.as_deref().unwrap_or("-")
    );
    println!(
        "  - blocked_on_permission={}",
        if viz_state.permission_blocked {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  - token_round_total={} token_session_total={} display={}",
        viz_state.latest_round_token_total,
        viz_state.session_token_total,
        if viz_state.show_usage_metrics {
            "ON"
        } else {
            "OFF"
        }
    );
    println!(
        "  - tools_total={} tools_failed={} tool_error_rate={}%",
        metrics.tool_calls_total,
        metrics.tool_calls_failed,
        metrics.tool_error_rate_percent()
    );
    println!(
        "  - tool_avg_duration_ms={}",
        metrics
            .avg_tool_duration_ms()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  - permission_waits={} error_events={}",
        metrics.permission_waits, metrics.error_events
    );
    println!();
}

fn extract_preview<'a>(message: &'a str, marker: &str) -> Option<&'a str> {
    let idx = message.find(marker)?;
    let start = idx + marker.len();
    let rest = message[start..].trim_start();
    let cut = rest.find('|').unwrap_or(rest.len());
    Some(rest[..cut].trim())
}

fn extract_tool_args_preview(message: &str) -> Option<&str> {
    extract_preview(message, "args_preview:")
}

fn extract_tool_result_preview(message: &str) -> Option<&str> {
    extract_preview(message, "result_preview:")
}

fn show_timeline(timeline: &[ndc_core::AgentExecutionEvent], limit: usize, mode: RedactionMode) {
    println!();
    println!("Recent Execution Timeline (last {}):", limit);
    let total = timeline.len();
    let start = total.saturating_sub(limit);
    if start == total {
        println!("  (empty)");
        println!();
        return;
    }
    let grouped = group_timeline_by_stage(&timeline[start..]);
    for (stage, events) in grouped {
        println!("  [stage:{}]", stage);
        for event in events {
            println!(
                "    - r{} {} | {}{}",
                event.round,
                event.timestamp.format("%H:%M:%S"),
                sanitize_text(&event.message, mode),
                event
                    .duration_ms
                    .map(|d| format!(" ({}ms)", d))
                    .unwrap_or_default()
            );
        }
    }
    println!();
}

async fn show_model_info(agent_manager: &AgentModeManager) {
    let status = agent_manager.status().await;
    println!("Current Model Configuration:");
    println!();
    println!("Provider: {}", status.provider);
    println!("Current model: {}", status.model);
    println!();
    match agent_manager.list_models(None).await {
        Ok(mut models) => {
            models.sort_by(|a, b| a.id.cmp(&b.id));
            if models.is_empty() {
                println!("No models returned by provider API.");
            } else {
                println!("Available models from provider API:");
                for m in models.iter().take(20) {
                    println!("  - {}", m.id);
                }
                if models.len() > 20 {
                    println!("  ... ({} total)", models.len());
                }
            }
        }
        Err(e) => {
            println!("[Error] Failed to fetch model list: {}", e);
        }
    }
    println!();
    println!("Usage:");
    println!("  /provider <name>           # switch provider");
    println!("  /model                     # list models for current provider");
    println!("  /model <model-id>          # switch model on current provider");
    println!("  /model <provider>/<model>  # backward compatible shortcut");
    println!();
    println!("Environment Variables:");
    println!("  NDC_OPENAI_API_KEY, NDC_OPENAI_MODEL");
    println!("  NDC_ANTHROPIC_API_KEY, NDC_ANTHROPIC_MODEL");
    println!("  NDC_MINIMAX_API_KEY, NDC_MINIMAX_MODEL  (applies to minimax* providers)");
    println!("  NDC_OPENROUTER_API_KEY, NDC_OPENROUTER_MODEL");
    println!("  NDC_OLLAMA_MODEL, NDC_OLLAMA_URL");
}

fn show_agent_status(status: &crate::agent_mode::AgentModeStatus) {
    println!();
    println!("+--------------------------------------------------------------------+");
    println!("|  Agent Status                                                        |");
    println!("+--------------------------------------------------------------------+");
    println!(
        "|  Status: {}                                                         ",
        if status.enabled {
            "Enabled"
        } else {
            "Disabled"
        }
    );
    if status.enabled {
        println!(
            "|  Agent: {}                                                         ",
            status.agent_name
        );
        println!(
            "|  Provider: {} @ {}                                                  ",
            status.provider, status.model
        );
        if let Some(sid) = &status.session_id {
            println!(
                "|  Session: {}                                                      ",
                sid
            );
        }
    }
    println!("+--------------------------------------------------------------------+");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock poisoned")
    }

    fn with_env_overrides<T>(updates: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
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

    fn mk_event(
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

    fn mk_event_at(
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

    fn render_event_snapshot(
        events: &[ndc_core::AgentExecutionEvent],
        viz: &mut ReplVisualizationState,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for event in events {
            out.extend(event_to_lines(event, viz));
        }
        out
    }

    fn line_plain(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn test_repl_state_default() {
        let state = ReplState::default();
        assert!(!state.session_id.is_empty());
        assert!(state.current_provider.is_none());
        assert!(state.current_model.is_none());
    }

    #[test]
    fn test_repl_state_not_expired() {
        let state = ReplState::default();
        assert!(!state.is_expired(3600));
        assert!(state.is_expired(0));
    }

    #[test]
    fn test_repl_config_default() {
        let config = ReplConfig::default();
        assert_eq!(config.prompt, "> ");
        assert!(!config.show_thought);
        assert_eq!(config.session_timeout, 3600);
    }

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
    fn test_sanitize_sensitive_text() {
        let input = "token=abc123 Bearer zyx987 sk-ABCDEF123456 /home/jerryg/project password:foo";
        let out = sanitize_text(input, RedactionMode::Basic);
        assert!(out.contains("token=[REDACTED]"));
        assert!(out.contains("Bearer [REDACTED]"));
        assert!(out.contains("sk-[REDACTED]"));
        assert!(out.contains("/home/***"));
        assert!(out.contains("password=[REDACTED]"));
        assert!(!out.contains("abc123"));
        assert!(!out.contains("zyx987"));
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
    fn test_show_recent_thinking_empty() {
        show_recent_thinking(&[], 10, RedactionMode::Basic);
    }

    #[test]
    fn test_push_log_line_capped() {
        let mut logs = Vec::new();
        for i in 0..(TUI_MAX_LOG_LINES + 5) {
            push_log_line(&mut logs, &format!("line-{}", i));
        }
        assert_eq!(logs.len(), TUI_MAX_LOG_LINES);
        assert_eq!(logs.first().map(String::as_str), Some("line-5"));
    }

    #[test]
    fn test_drain_live_execution_events_renders_matching_session() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        // Use ToolCallStart which produces output in all verbosity modes
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
        let mut logs = Vec::new();
        let rendered =
            drain_live_execution_events(&mut recv, Some("session-a"), &mut viz, &mut logs);
        assert!(rendered);
        assert_eq!(viz.timeline_cache.len(), 1);
        assert_eq!(viz.timeline_cache[0].message, event.message);
        assert!(
            logs.iter()
                .any(|l| l.contains("[ToolRun]") && l.contains("read"))
        );
    }

    #[test]
    fn test_drain_live_execution_events_ignores_other_sessions() {
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
        let mut logs = Vec::new();
        let rendered =
            drain_live_execution_events(&mut recv, Some("session-a"), &mut viz, &mut logs);
        assert!(!rendered);
        assert!(viz.timeline_cache.is_empty());
        assert!(logs.is_empty());
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
            "workflow_stage: discovery | tool_calls_planned",
            2,
            None,
            None,
            None,
            false,
        );
        let lines = event_to_lines(&event, &mut viz);
        assert_eq!(viz.current_workflow_stage.as_deref(), Some("discovery"));
        assert_eq!(viz.current_workflow_stage_index, Some(2));
        assert_eq!(
            viz.current_workflow_stage_total,
            Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES)
        );
        assert!(viz.current_workflow_stage_started_at.is_some());
        assert!(!viz.permission_blocked);
        // Compact mode: single [Stage] line
        assert!(lines.iter().any(|line| line.contains("[Stage]")));
        assert!(lines.iter().any(|line| line.contains("Discovery")));
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
        assert!(joined.contains("Workflow Overview (verbose) current=executing progress=60%(3/5)"));
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
    fn test_env_char_default_and_override() {
        with_env_overrides(&[("NDC_REPL_KEY_TOGGLE_THINKING", None)], || {
            assert_eq!(env_char("NDC_REPL_KEY_TOGGLE_THINKING", 't'), 't');
            unsafe {
                std::env::set_var("NDC_REPL_KEY_TOGGLE_THINKING", "X");
            }
            assert_eq!(env_char("NDC_REPL_KEY_TOGGLE_THINKING", 't'), 'x');
        });
    }

    #[test]
    fn test_keymap_hint() {
        let map = ReplTuiKeymap {
            toggle_thinking: 't',
            toggle_details: 'd',
            toggle_tool_cards: 'e',
            show_recent_thinking: 'y',
            show_timeline: 'i',
            clear_panel: 'l',
        };
        let hint = map.hint();
        assert!(hint.contains("Ctrl+T"));
        assert!(hint.contains("Ctrl+D"));
        assert!(hint.contains("Ctrl+E"));
        assert!(hint.contains("Ctrl+Y"));
        assert!(hint.contains("Ctrl+I"));
        assert!(hint.contains("Ctrl+L"));
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
    fn test_handle_session_scroll_key_page_navigation() {
        use crossterm::event::KeyEvent;
        let mut view = TuiSessionViewState {
            scroll_offset: 0,
            auto_follow: true,
            body_height: 10,
        };
        assert!(handle_session_scroll_key(
            &KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &mut view,
            30
        ));
        assert_eq!(view.scroll_offset, 15);
        assert!(!view.auto_follow);
        assert!(handle_session_scroll_key(
            &KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut view,
            30
        ));
        assert_eq!(view.scroll_offset, 20);
        assert!(view.auto_follow);
        assert!(handle_session_scroll_key(
            &KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            &mut view,
            30
        ));
        assert_eq!(view.scroll_offset, 20);
        assert!(view.auto_follow);
    }

    #[test]
    fn test_build_input_hint_line_for_slash() {
        let hints = build_input_hint_lines("/", None);
        let joined = hints.join(" ");
        assert!(joined.contains("/help"));
        assert!(joined.contains("/provider"));
        assert!(joined.contains("Tab"));
    }

    #[test]
    fn test_apply_slash_completion_cycles_matches() {
        let mut input = "/p".to_string();
        let mut state = None;
        assert!(apply_slash_completion(&mut input, &mut state, false));
        assert_eq!(input, "/provider");
        assert!(apply_slash_completion(&mut input, &mut state, false));
        assert_eq!(input, "/providers");
        assert!(apply_slash_completion(&mut input, &mut state, true));
        assert_eq!(input, "/provider");
    }

    #[test]
    fn test_apply_slash_completion_provider_argument() {
        let mut input = "/provider ".to_string();
        let mut state = None;
        assert!(apply_slash_completion(&mut input, &mut state, false));
        assert_eq!(input, "/provider openai");
        assert!(apply_slash_completion(&mut input, &mut state, false));
        assert_eq!(input, "/provider anthropic");
    }

    #[test]
    fn test_build_input_hint_line_selected_entry() {
        let selected = ReplCommandCompletionState {
            suggestions: vec![
                "/help".to_string(),
                "/provider".to_string(),
                "/providers".to_string(),
            ],
            selected_index: 1,
        };
        let hints = build_input_hint_lines("/", Some(&selected));
        let joined = hints.join(" ");
        assert!(joined.contains("Selected [2/3]"));
        assert!(joined.contains("/provider"));
    }

    #[test]
    fn test_build_input_hint_line_provider_options() {
        let hints = build_input_hint_lines("/provider ", None);
        let joined = hints.join(" ");
        assert!(joined.contains("openai"));
        assert!(joined.contains("anthropic"));
        assert!(joined.contains("ollama"));
    }

    #[test]
    fn test_build_input_hint_line_workflow_options() {
        let hints = build_input_hint_lines("/workflow ", None);
        let joined = hints.join(" ");
        assert!(joined.contains("compact"));
        assert!(joined.contains("verbose"));
    }

    #[test]
    fn test_apply_slash_completion_workflow_argument() {
        let mut input = "/workflow ".to_string();
        let mut state = None;
        assert!(apply_slash_completion(&mut input, &mut state, false));
        assert_eq!(input, "/workflow compact");
        assert!(apply_slash_completion(&mut input, &mut state, false));
        assert_eq!(input, "/workflow verbose");
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
    fn test_handle_session_scroll_mouse() {
        let mut view = TuiSessionViewState {
            scroll_offset: 0,
            auto_follow: true,
            body_height: 10,
        };
        let up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        assert!(handle_session_scroll_mouse(&up, &mut view, 30));
        assert_eq!(view.scroll_offset, 17);
        assert!(!view.auto_follow);
        let down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        assert!(handle_session_scroll_mouse(&down, &mut view, 30));
        assert_eq!(view.scroll_offset, 20);
        assert!(view.auto_follow);
    }

    #[test]
    fn test_short_session_id() {
        assert_eq!(short_session_id(None), "-");
        assert_eq!(short_session_id(Some("abc")), "abc");
        assert_eq!(short_session_id(Some("1234567890abcdef")), "1234567890ab…");
    }

    #[test]
    fn test_build_status_line_contains_session() {
        let status = crate::agent_mode::AgentModeStatus {
            enabled: true,
            agent_name: "build".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            session_id: Some("agent-1234567890abcdef".to_string()),
            active_task_id: None,
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
        let status = crate::agent_mode::AgentModeStatus {
            enabled: true,
            agent_name: "build".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            session_id: Some("agent-1".to_string()),
            active_task_id: None,
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
    fn test_style_session_log_line_tool_and_partitions() {
        let theme = TuiTheme::default_dark();
        let tool = style_session_log_line("[Tool][r1] failed read (3ms)", &theme);
        assert_eq!(tool.spans[0].content.as_ref(), "  ✗ ");
        assert_eq!(tool.spans[0].style.fg, Some(Color::Red));

        let input = style_session_log_line("  ├─ input : {\"path\":\"README.md\"}", &theme);
        assert_eq!(line_plain(&input), "  ├─ input : {\"path\":\"README.md\"}");
        assert_eq!(input.spans[1].content.as_ref(), "input");
        assert_eq!(input.spans[1].style.fg, Some(Color::Cyan));

        let output = style_session_log_line("  ├─ output: ok", &theme);
        assert_eq!(line_plain(&output), "  ├─ output: ok");
        assert_eq!(output.spans[1].content.as_ref(), "output");
        assert_eq!(output.spans[1].style.fg, Some(Color::Green));
    }

    #[test]
    fn test_detect_tui_shortcut_actions() {
        use crossterm::event::KeyEvent;
        let map = ReplTuiKeymap {
            toggle_thinking: 't',
            toggle_details: 'd',
            toggle_tool_cards: 'e',
            show_recent_thinking: 'y',
            show_timeline: 'i',
            clear_panel: 'l',
        };
        assert_eq!(
            detect_tui_shortcut(
                &KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
                &map
            ),
            Some(TuiShortcutAction::ToggleThinking)
        );
        assert_eq!(
            detect_tui_shortcut(
                &KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
                &map
            ),
            Some(TuiShortcutAction::ToggleDetails)
        );
        assert_eq!(
            detect_tui_shortcut(
                &KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
                &map
            ),
            Some(TuiShortcutAction::ToggleToolCards)
        );
        assert_eq!(
            detect_tui_shortcut(
                &KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
                &map
            ),
            Some(TuiShortcutAction::ShowRecentThinking)
        );
        assert_eq!(
            detect_tui_shortcut(
                &KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
                &map
            ),
            Some(TuiShortcutAction::ShowTimeline)
        );
        assert_eq!(
            detect_tui_shortcut(
                &KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
                &map
            ),
            Some(TuiShortcutAction::ClearPanel)
        );
        assert_eq!(
            detect_tui_shortcut(
                &KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
                &map
            ),
            None
        );
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

    // ===== P1-UX Feature Tests =====

    // --- input_line_count ---

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

    // --- tui_layout_constraints with input_lines ---

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

    // --- format_token_count ---

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

    // --- token_progress_bar ---

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

    // --- truncate_output ---

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

    // --- parse_inline_spans ---

    #[test]
    fn test_parse_inline_spans_plain() {
        let theme = TuiTheme::default_dark();
        let spans = parse_inline_spans("hello world", &theme);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "hello world");
    }

    #[test]
    fn test_parse_inline_spans_backtick_code() {
        let theme = TuiTheme::default_dark();
        let spans = parse_inline_spans("use `cargo build` here", &theme);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "use ");
        assert_eq!(spans[1].content.as_ref(), "cargo build");
        assert_eq!(spans[2].content.as_ref(), " here");
        // code span should have background color
        assert!(spans[1].style.bg.is_some());
    }

    #[test]
    fn test_parse_inline_spans_bold() {
        let theme = TuiTheme::default_dark();
        let spans = parse_inline_spans("this is **bold** text", &theme);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].content.as_ref(), "bold");
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_parse_inline_spans_italic() {
        let theme = TuiTheme::default_dark();
        let spans = parse_inline_spans("this is *italic* text", &theme);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].content.as_ref(), "italic");
        assert!(spans[1].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_parse_inline_spans_unclosed_backtick() {
        let theme = TuiTheme::default_dark();
        let spans = parse_inline_spans("unclosed `code", &theme);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "unclosed ");
        assert_eq!(spans[1].content.as_ref(), "`code");
    }

    // --- render_inline_markdown ---

    #[test]
    fn test_render_inline_markdown_header_h2() {
        let theme = TuiTheme::default_dark();
        let line = render_inline_markdown("## Section Title", &theme);
        let text = line_plain(&line);
        assert!(text.contains("Section Title"));
    }

    #[test]
    fn test_render_inline_markdown_bullet_dash() {
        let theme = TuiTheme::default_dark();
        let line = render_inline_markdown("- list item", &theme);
        let text = line_plain(&line);
        assert!(text.contains("•"));
        assert!(text.contains("list item"));
    }

    #[test]
    fn test_render_inline_markdown_bullet_star() {
        let theme = TuiTheme::default_dark();
        let line = render_inline_markdown("* another item", &theme);
        let text = line_plain(&line);
        assert!(text.contains("•"));
        assert!(text.contains("another item"));
    }

    #[test]
    fn test_render_inline_markdown_code_fence() {
        let theme = TuiTheme::default_dark();
        let line = render_inline_markdown("```rust", &theme);
        let text = line_plain(&line);
        assert!(text.contains("```rust"));
    }

    #[test]
    fn test_render_inline_markdown_plain_with_inline_code() {
        let theme = TuiTheme::default_dark();
        let line = render_inline_markdown("run `cargo test`", &theme);
        let text = line_plain(&line);
        assert_eq!(text, "run cargo test");
    }

    #[test]
    fn test_render_inline_markdown_preserves_indent() {
        let theme = TuiTheme::default_dark();
        let line = render_inline_markdown("    indented text", &theme);
        let text = line_plain(&line);
        assert!(text.starts_with("    "));
    }

    // --- style_session_log_line truncation ---

    #[test]
    fn test_style_session_log_line_output_truncation() {
        let long_output = format!("  ├─ output: {}", "x".repeat(300));
        let theme = TuiTheme::default_dark();
        let line = style_session_log_line(&long_output, &theme);
        let text = line_plain(&line);
        assert!(text.contains("truncated"));
        assert!(text.len() < 300);
    }

    #[test]
    fn test_style_session_log_line_output_no_truncation_short() {
        let theme = TuiTheme::default_dark();
        let line = style_session_log_line("  ├─ output: ok", &theme);
        let text = line_plain(&line);
        assert!(!text.contains("truncated"));
        assert!(text.contains("ok"));
    }

    // --- InputHistory multiline ---

    #[test]
    fn test_input_history_multiline_entries() {
        let mut hist = InputHistory::new(10);
        hist.push("line1\nline2".to_string());
        hist.push("single".to_string());
        assert_eq!(hist.entries.len(), 2);

        // Navigate up to get latest
        let up1 = hist.up("current");
        assert_eq!(up1, Some("single"));
        let up2 = hist.up("");
        assert_eq!(up2, Some("line1\nline2"));
    }

    #[test]
    fn test_input_history_down_restores_draft() {
        let mut hist = InputHistory::new(10);
        hist.push("old".to_string());

        hist.up("my draft");
        let down = hist.down();
        assert_eq!(down, Some("my draft"));
    }

    // --- Permission message lifecycle ---

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

    // ===== P1-UX-6 Tests: Verbosity & Display =====

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
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Discovery),
            workflow_detail: Some("scanning files".to_string()),
            workflow_stage_index: Some(2),
            workflow_stage_total: Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES),
        };
        let lines = event_to_lines(&event, &mut viz);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Discovery") && l.contains("scanning files"))
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

    #[test]
    fn test_verbosity_env_override() {
        with_env_overrides(&[("NDC_DISPLAY_VERBOSITY", Some("normal"))], || {
            let state = ReplVisualizationState::new(false);
            assert!(matches!(state.verbosity, DisplayVerbosity::Normal));
        });
    }

    // ===== P1-UX-2 Chat Turn Model Tests =====

    fn render_entries_snapshot(
        events: &[ndc_core::AgentExecutionEvent],
        viz: &mut ReplVisualizationState,
    ) -> Vec<ChatEntry> {
        let mut out = Vec::new();
        for event in events {
            out.extend(event_to_entries(event, viz));
        }
        out
    }

    fn entry_lines_plain(entry: &ChatEntry) -> Vec<String> {
        let theme = TuiTheme::default_dark();
        let mut lines = Vec::new();
        style_chat_entry(entry, &theme, &mut lines);
        lines.iter().map(line_plain).collect()
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
            round: 1,
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
            round: 1,
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
            round: 1,
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
            round: 1,
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
                round: 1,
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
                round: 1,
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
                round: 1,
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
                round: 1,
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
                round: 1,
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
    fn test_chat_turn_grouping() {
        let turn = ChatTurn {
            turn_id: 1,
            entries: vec![
                ChatEntry::UserMessage {
                    content: "hello".to_string(),
                    turn_id: 1,
                },
                ChatEntry::SystemNote("processing...".to_string()),
                ChatEntry::AssistantMessage {
                    content: "hi".to_string(),
                    turn_id: 1,
                },
            ],
        };
        assert_eq!(turn.turn_id, 1);
        assert_eq!(turn.entries.len(), 3);
        // Rendering the turn's entries should work
        let lines = style_chat_entries(&turn.entries);
        let plain: Vec<String> = lines.iter().map(line_plain).collect();
        assert!(plain.iter().any(|l| l.contains("You [#1]")));
        assert!(plain.iter().any(|l| l.contains("hi")));
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
