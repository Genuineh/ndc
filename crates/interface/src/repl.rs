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
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, style::Stylize};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::Terminal;

// Agent mode integration
use crate::agent_mode::{handle_agent_command, AgentModeConfig, AgentModeManager};
use crate::redaction::{sanitize_text, RedactionMode};

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
        Self {
            show_thinking,
            show_tool_details,
            expand_tool_cards,
            live_events_enabled,
            show_usage_metrics,
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
    if has_live_receiver {
        "live"
    } else {
        "poll"
    }
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

fn tui_layout_constraints() -> [Constraint; 4] {
    [
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(4),
        Constraint::Length(4),
    ]
}

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
        KeyCode::Up => {
            move_session_scroll(session_view, log_count, -1);
            true
        }
        KeyCode::Down => {
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
    logs.iter()
        .map(|line| style_session_log_line(line))
        .collect()
}

fn style_session_log_line(line: &str) -> Line<'static> {
    let plain = || Line::from(Span::raw(line.to_string()));
    let muted = Style::default().fg(Color::DarkGray);
    let subtle = Style::default().fg(Color::Gray);
    let title = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let success = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let warning = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let danger = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);

    if line == "You:" {
        return Line::from(Span::styled(
            "You:",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if line == "Assistant:" {
        return Line::from(Span::styled("Assistant:", title));
    }
    if let Some(text) = line.strip_prefix("You: ") {
        return Line::from(vec![
            Span::styled(
                "You:",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::raw(text.to_string()),
        ]);
    }
    if line.starts_with("[Agent] processing") {
        return Line::from(Span::styled(line.to_string(), warning));
    }
    if line.starts_with("[Error]") || line.starts_with("[Error][") {
        return Line::from(Span::styled(line.to_string(), danger));
    }
    if line.starts_with("[Permission]") {
        return Line::from(Span::styled(line.to_string(), warning));
    }
    if line.starts_with("[Tool]") {
        let style = if line.contains(" failed ") {
            danger
        } else if line.contains(" done ") {
            success
        } else {
            title
        };
        return Line::from(Span::styled(line.to_string(), style));
    }
    if line.starts_with("[Workflow]") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if line.starts_with("[Usage]") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::LightGreen),
        ));
    }
    if line.starts_with("[Thinking]") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Magenta),
        ));
    }
    if line.starts_with("[Step]") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Cyan),
        ));
    }
    if line.trim_start().starts_with("[stage:") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if line.starts_with("[Agent][") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Yellow),
        ));
    }
    if line.starts_with("[OK]") {
        return Line::from(Span::styled(line.to_string(), success));
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
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" : ", subtle),
            Span::raw(value.to_string()),
        ]);
    }
    if let Some(value) = line.strip_prefix("  ├─ output: ") {
        return Line::from(vec![
            Span::styled("  ├─ ", subtle),
            Span::styled(
                "output",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(": ", subtle),
            Span::raw(value.to_string()),
        ]);
    }
    if let Some(value) = line.strip_prefix("  ├─ error : ") {
        return Line::from(vec![
            Span::styled("  ├─ ", subtle),
            Span::styled(
                "error",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
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
                    .fg(Color::LightMagenta)
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
    plain()
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

    let mut logs: Vec<String> = vec![
        "NDC - Neo Development Companion".to_string(),
        "Type natural language and press Enter. /help for commands.".to_string(),
    ];
    let keymap = ReplTuiKeymap::from_env();
    logs.push(format!("Shortcuts: {}", keymap.hint()));

    let mut input = String::new();
    let mut completion_state: Option<ReplCommandCompletionState> = None;
    let mut processing_handle: Option<
        tokio::task::JoinHandle<Result<ndc_core::AgentResponse, ndc_core::AgentError>>,
    > = None;
    let mut streamed_count = 0usize;
    let mut streamed_any = false;
    let mut last_poll = Instant::now();
    let mut should_quit = false;
    let mut session_view = TuiSessionViewState::default();
    let mut live_events: Option<
        tokio::sync::broadcast::Receiver<ndc_core::AgentSessionExecutionEvent>,
    > = None;
    let mut live_session_id: Option<String> = None;

    while !should_quit {
        if viz_state.live_events_enabled
            && drain_live_execution_events(
                &mut live_events,
                live_session_id.as_deref(),
                viz_state,
                &mut logs,
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
        let status_line = build_status_line(
            &status,
            viz_state,
            is_processing,
            &session_view,
            stream_state,
        );
        let hint_lines = build_input_hint_lines(input.as_str(), completion_state.as_ref());

        terminal.draw(|f| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints(tui_layout_constraints())
                .split(f.area());

            let status_widget = Paragraph::new(Line::from(status_line.clone().cyan().to_string()));
            f.render_widget(status_widget, areas[0]);

            let body_block = Block::default().title("Session").borders(Borders::ALL);
            let inner = body_block.inner(areas[1]);
            session_view.body_height = (inner.height as usize).max(1);
            let scroll = effective_log_scroll(logs.len(), &session_view) as u16;
            let body = Paragraph::new(Text::from(style_session_log_lines(logs.as_slice())))
                .block(body_block)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0));
            f.render_widget(body, areas[1]);
            if logs.len() > session_view.body_height {
                let mut scrollbar_state = ScrollbarState::new(logs.len())
                    .position(effective_log_scroll(logs.len(), &session_view));
                let scrollbar = Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight)
                    .thumb_style(Style::default().fg(Color::Gray));
                f.render_stateful_widget(scrollbar, areas[1], &mut scrollbar_state);
            }

            let hints_widget = Paragraph::new(Text::from(
                hint_lines
                    .iter()
                    .cloned()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            ))
            .block(Block::default().title("Hints").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
            f.render_widget(hints_widget, areas[2]);

            let input_title =
                "Input (/workflow /tokens /metrics /t /d /cards /stream /timeline /clear, Enter send, Esc exit, ↑↓/PgUp/PgDn scroll, Ctrl+<keys>)";
            let input_widget = Paragraph::new(Line::from(format!("> {}", input)))
                .block(Block::default().title(input_title).borders(Borders::ALL));
            f.render_widget(input_widget, areas[3]);
            let x = areas[3].x + 2 + input.len() as u16;
            let y = areas[3].y + 1;
            f.set_cursor_position((x, y));
        })?;

        if let Some(handle) = processing_handle.as_ref() {
            if live_events.is_none() && last_poll.elapsed() >= Duration::from_millis(120) {
                if let Ok(events) = agent_manager
                    .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
                    .await
                    && events.len() > streamed_count {
                        let new_events = &events[streamed_count..];
                        append_timeline_events(
                            &mut viz_state.timeline_cache,
                            new_events,
                            TIMELINE_CACHE_MAX_EVENTS,
                        );
                        for event in new_events {
                            for line in event_to_lines(event, viz_state) {
                                push_log_line(&mut logs, &line);
                            }
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
                                for line in event_to_lines(event, viz_state) {
                                    push_log_line(&mut logs, &line);
                                }
                            }
                        }
                        if !response.content.trim().is_empty() {
                            push_log_line(&mut logs, "");
                            push_log_line(&mut logs, "Assistant:");
                            for line in response.content.lines() {
                                push_log_line(&mut logs, &format!("  {}", line));
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        push_log_line(&mut logs, &format!("[Error] {}", e));
                    }
                    Err(e) => {
                        push_log_line(&mut logs, &format!("[Error] join failed: {}", e));
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

                    if handle_session_scroll_key(&key, &mut session_view, logs.len()) {
                        continue;
                    }

                    if let Some(action) = detect_tui_shortcut(&key, &keymap) {
                        apply_tui_shortcut_action(action, viz_state, logs.as_mut());
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
                        KeyCode::Enter => {
                            let cmd = input.trim().to_string();
                            input.clear();
                            completion_state = None;
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
                                    &mut logs,
                                )
                                .await?
                                {
                                    should_quit = true;
                                }
                                continue;
                            }

                            push_log_line(&mut logs, "");
                            push_log_line(&mut logs, &format!("You: {}", cmd));
                            push_log_line(&mut logs, "[Agent] processing...");
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
                                        push_log_line(
                                            &mut logs,
                                            &format!(
                                                "[Warning] realtime stream unavailable: {}",
                                                e
                                            ),
                                        );
                                    }
                                }
                            } else {
                                push_log_line(
                                    &mut logs,
                                    "[Tip] realtime stream is OFF, using polling fallback",
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
                    let _ = handle_session_scroll_mouse(&mouse, &mut session_view, logs.len());
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
            && events.len() > streamed_count {
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
        && events.len() > streamed_count {
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
  /stream [mode]  Toggle realtime event stream (on/off/status)
  /workflow [mode] Show workflow overview (compact|verbose; default verbose)
  /tokens [mode]  Token metrics: show/hide/reset/status
  /metrics        Runtime metrics (tools/errors/permission/tokens)
  /timeline [N]   Show recent execution timeline (default N=40)
  /clear          Clear screen
  exit, quit, q   Exit REPL

TUI Shortcuts:
  Ctrl+T          Toggle thinking
  Ctrl+D          Toggle tool details
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
    }
    match event.kind {
        ndc_core::AgentExecutionEventKind::WorkflowStage => {
            if let Some(stage_info) = event.workflow_stage_info() {
                let stage = stage_info.stage;
                viz_state.current_workflow_stage = Some(stage.as_str().to_string());
                viz_state.current_workflow_stage_index = Some(stage_info.index);
                viz_state.current_workflow_stage_total = Some(stage_info.total);
                viz_state.current_workflow_stage_started_at = Some(event.timestamp);
                return vec![
                    format!("[stage:{}]", stage),
                    format!(
                        "[Workflow][r{}] {}",
                        event.round,
                        sanitize_text(&event.message, viz_state.redaction_mode)
                    ),
                ];
            }
            vec![format!(
                "[Workflow][r{}] {}",
                event.round,
                sanitize_text(&event.message, viz_state.redaction_mode)
            )]
        }
        ndc_core::AgentExecutionEventKind::Reasoning => {
            if viz_state.show_thinking {
                vec![
                    format!("[Thinking][r{}]", event.round),
                    format!(
                        "  └─ {}",
                        sanitize_text(&event.message, viz_state.redaction_mode)
                    ),
                ]
            } else if !viz_state.hidden_thinking_round_hints.contains(&event.round) {
                viz_state.hidden_thinking_round_hints.insert(event.round);
                vec![format!(
                    "[Thinking][r{}] (collapsed, use /t or /thinking show)",
                    event.round
                )]
            } else {
                Vec::new()
            }
        }
        ndc_core::AgentExecutionEventKind::ToolCallStart => {
            let mut lines = vec![format!(
                "[Tool][r{}] start {}",
                event.round,
                event.tool_name.as_deref().unwrap_or("unknown")
            )];
            if viz_state.expand_tool_cards
                && let Some(args) = extract_tool_args_preview(&event.message) {
                    lines.push(format!(
                        "  └─ input : {}",
                        sanitize_text(args, viz_state.redaction_mode)
                    ));
                }
            lines
        }
        ndc_core::AgentExecutionEventKind::ToolCallEnd => {
            let mut lines = vec![format!(
                "[Tool][r{}] {} {}{}",
                event.round,
                if event.is_error { "failed" } else { "done" },
                event.tool_name.as_deref().unwrap_or("unknown"),
                event
                    .duration_ms
                    .map(|d| format!(" ({}ms)", d))
                    .unwrap_or_default()
            )];
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
            lines
        }
        ndc_core::AgentExecutionEventKind::TokenUsage => {
            if let Some(usage) = event.token_usage_info() {
                viz_state.latest_round_token_total = usage.total_tokens;
                viz_state.session_token_total = usage.session_total;
            }
            vec![format!(
                "[Usage][r{}] {}",
                event.round,
                sanitize_text(&event.message, viz_state.redaction_mode)
            )]
        }
        ndc_core::AgentExecutionEventKind::PermissionAsked => {
            viz_state.permission_blocked = true;
            vec![format!(
                "[Permission][r{}] {}",
                event.round,
                sanitize_text(&event.message, viz_state.redaction_mode)
            )]
        }
        ndc_core::AgentExecutionEventKind::StepStart
        | ndc_core::AgentExecutionEventKind::StepFinish
        | ndc_core::AgentExecutionEventKind::Verification => {
            if !viz_state.show_tool_details
                && matches!(event.kind, ndc_core::AgentExecutionEventKind::StepStart)
            {
                return vec![format!("[Agent][r{}] thinking...", event.round)];
            }
            if viz_state.show_tool_details {
                return vec![format!(
                    "[Step][r{}] {}{}",
                    event.round,
                    sanitize_text(&event.message, viz_state.redaction_mode),
                    event
                        .duration_ms
                        .map(|d| format!(" ({}ms)", d))
                        .unwrap_or_default()
                )];
            }
            Vec::new()
        }
        ndc_core::AgentExecutionEventKind::Error => vec![format!(
            "[Error][r{}] {}",
            event.round,
            sanitize_text(&event.message, viz_state.redaction_mode)
        )],
        ndc_core::AgentExecutionEventKind::SessionStatus
        | ndc_core::AgentExecutionEventKind::Text => Vec::new(),
    }
}

fn append_recent_thinking_to_logs(logs: &mut Vec<String>, viz_state: &ReplVisualizationState) {
    let total = viz_state.timeline_cache.len();
    let start = total.saturating_sub(viz_state.timeline_limit);
    push_log_line(logs, "");
    push_log_line(
        logs,
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
        push_log_line(
            logs,
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
        push_log_line(logs, "  (no thinking events yet)");
    }
}

fn append_recent_timeline_to_logs(logs: &mut Vec<String>, viz_state: &ReplVisualizationState) {
    push_log_line(logs, "");
    push_log_line(
        logs,
        &format!(
            "Recent Execution Timeline (last {}):",
            viz_state.timeline_limit
        ),
    );
    let total = viz_state.timeline_cache.len();
    let start = total.saturating_sub(viz_state.timeline_limit);
    if start == total {
        push_log_line(logs, "  (empty)");
        return;
    }
    let grouped = group_timeline_by_stage(&viz_state.timeline_cache[start..]);
    for (stage, events) in grouped {
        push_log_line(logs, &format!("  [stage:{}]", stage));
        for event in events {
            push_log_line(
                logs,
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

fn append_workflow_overview_to_logs(
    logs: &mut Vec<String>,
    viz_state: &ReplVisualizationState,
    mode: WorkflowOverviewMode,
) {
    push_log_line(logs, "");
    push_log_line(
        logs,
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
        push_log_line(
            logs,
            &format!(
                "[Warning] workflow history may be partial (cache cap={} events)",
                TIMELINE_CACHE_MAX_EVENTS
            ),
        );
    }
    push_log_line(logs, "Workflow Progress:");
    for stage in WORKFLOW_STAGE_ORDER {
        let metrics = summary.stages.get(*stage).copied().unwrap_or_default();
        let active_ms = if summary.current_stage.as_deref() == Some(*stage) {
            summary.current_stage_active_ms
        } else {
            0
        };
        push_log_line(
            logs,
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
            push_log_line(
                logs,
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
            push_log_line(logs, "  (no workflow stage events yet)");
        }
    } else {
        push_log_line(
            logs,
            "  (use /workflow verbose to inspect stage event timeline)",
        );
    }
}

fn append_token_usage_to_logs(logs: &mut Vec<String>, viz_state: &ReplVisualizationState) {
    push_log_line(logs, "");
    push_log_line(
        logs,
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

fn append_runtime_metrics_to_logs(logs: &mut Vec<String>, viz_state: &ReplVisualizationState) {
    let metrics = compute_runtime_metrics(viz_state.timeline_cache.as_slice());
    push_log_line(logs, "");
    push_log_line(logs, "Runtime Metrics:");
    push_log_line(
        logs,
        &format!(
            "  - workflow_current={}",
            viz_state.current_workflow_stage.as_deref().unwrap_or("-")
        ),
    );
    push_log_line(
        logs,
        &format!(
            "  - blocked_on_permission={}",
            if viz_state.permission_blocked {
                "yes"
            } else {
                "no"
            }
        ),
    );
    push_log_line(
        logs,
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
    push_log_line(
        logs,
        &format!(
            "  - tools_total={} tools_failed={} tool_error_rate={}%",
            metrics.tool_calls_total,
            metrics.tool_calls_failed,
            metrics.tool_error_rate_percent()
        ),
    );
    push_log_line(
        logs,
        &format!(
            "  - tool_avg_duration_ms={}",
            metrics
                .avg_tool_duration_ms()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
    );
    push_log_line(
        logs,
        &format!(
            "  - permission_waits={} error_events={}",
            metrics.permission_waits, metrics.error_events
        ),
    );
}

fn apply_tui_shortcut_action(
    action: TuiShortcutAction,
    viz_state: &mut ReplVisualizationState,
    logs: &mut Vec<String>,
) {
    match action {
        TuiShortcutAction::ToggleThinking => {
            viz_state.show_thinking = !viz_state.show_thinking;
            if viz_state.show_thinking {
                viz_state.hidden_thinking_round_hints.clear();
            }
            push_log_line(
                logs,
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
            viz_state.show_tool_details = !viz_state.show_tool_details;
            push_log_line(
                logs,
                &format!(
                    "[OK] Details: {}",
                    if viz_state.show_tool_details {
                        "ON"
                    } else {
                        "OFF"
                    }
                ),
            );
        }
        TuiShortcutAction::ToggleToolCards => {
            viz_state.expand_tool_cards = !viz_state.expand_tool_cards;
            push_log_line(
                logs,
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
            append_recent_thinking_to_logs(logs, viz_state);
        }
        TuiShortcutAction::ShowTimeline => {
            append_recent_timeline_to_logs(logs, viz_state);
        }
        TuiShortcutAction::ClearPanel => {
            logs.clear();
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

async fn restore_session_logs_to_panel(
    agent_manager: &Arc<AgentModeManager>,
    viz_state: &mut ReplVisualizationState,
    logs: &mut Vec<String>,
) {
    match agent_manager
        .session_timeline(Some(TIMELINE_CACHE_MAX_EVENTS))
        .await
    {
        Ok(events) if !events.is_empty() => {
            viz_state.timeline_cache = events.clone();
            push_log_line(logs, "--- Restored session history ---");
            for event in &events {
                for line in event_to_lines(event, viz_state) {
                    push_log_line(logs, &line);
                }
            }
            push_log_line(logs, "---");
        }
        Ok(_) => {}
        Err(e) => push_log_line(
            logs,
            &format!("[Warning] Could not restore session history: {}", e),
        ),
    }
}

async fn handle_tui_command(
    input: &str,
    viz_state: &mut ReplVisualizationState,
    agent_manager: Arc<AgentModeManager>,
    logs: &mut Vec<String>,
) -> io::Result<bool> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts[0] {
        "/help" | "/h" => {
            push_log_line(
                logs,
                "Commands: /help /provider /model /status /workflow /tokens /metrics /t /d /cards /stream /thinking /timeline [N] /copy /resume [id] [--cross] /new /session [N] /project [dir] /clear /exit",
            );
            push_log_line(
                logs,
                "Shortcuts: Ctrl+T / Ctrl+D / Ctrl+E / Ctrl+Y / Ctrl+I / Ctrl+L",
            );
            push_log_line(
                logs,
                "Scroll: Up/Down line, PgUp/PgDn half-page, Home/End top-bottom, drag to select",
            );
        }
        "/thinking" | "/t" => {
            if parts.len() > 1 && (parts[1] == "show" || parts[1] == "now") {
                append_recent_thinking_to_logs(logs, viz_state);
            } else {
                viz_state.show_thinking = !viz_state.show_thinking;
                if viz_state.show_thinking {
                    viz_state.hidden_thinking_round_hints.clear();
                }
                push_log_line(
                    logs,
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
            push_log_line(
                logs,
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
            push_log_line(
                logs,
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
        "/stream" => match apply_stream_command(viz_state, parts.get(1).copied()) {
            Ok(message) => push_log_line(logs, &format!("[OK] {}", message)),
            Err(message) => push_log_line(logs, &format!("[Error] {}", message)),
        },
        "/workflow" => {
            let mode = match WorkflowOverviewMode::parse(parts.get(1).copied()) {
                Ok(mode) => mode,
                Err(message) => {
                    push_log_line(logs, &format!("[Error] {}", message));
                    return Ok(false);
                }
            };
            append_workflow_overview_to_logs(logs, viz_state, mode);
        }
        "/tokens" => match apply_tokens_command(viz_state, parts.get(1).copied()) {
            Ok(message) => {
                push_log_line(logs, &format!("[OK] {}", message));
                append_token_usage_to_logs(logs, viz_state);
            }
            Err(message) => push_log_line(logs, &format!("[Error] {}", message)),
        },
        "/metrics" => {
            append_runtime_metrics_to_logs(logs, viz_state);
        }
        "/timeline" => {
            if parts.len() > 1
                && let Ok(parsed) = parts[1].parse::<usize>() {
                    viz_state.timeline_limit = parsed.max(1);
                }
            match agent_manager
                .session_timeline(Some(viz_state.timeline_limit))
                .await
            {
                Ok(events) => {
                    viz_state.timeline_cache = events;
                    append_recent_timeline_to_logs(logs, viz_state);
                }
                Err(e) => push_log_line(logs, &format!("[Warning] {}", e)),
            }
        }
        "/provider" | "/providers" | "/p" => {
            if parts.len() > 1 {
                if let Err(e) = agent_manager.switch_provider(parts[1], None).await {
                    push_log_line(logs, &format!("[Error] {}", e));
                } else {
                    let status = agent_manager.status().await;
                    push_log_line(
                        logs,
                        &format!(
                            "[OK] Provider switched to '{}' with model '{}'",
                            status.provider, status.model
                        ),
                    );
                }
            } else {
                let status = agent_manager.status().await;
                push_log_line(logs, &format!("Current provider: {}", status.provider));
                push_log_line(
                    logs,
                    &format!("Available providers: {}", AVAILABLE_PROVIDERS.join(", ")),
                );
                push_log_line(logs, "Usage: /provider <name>");
            }
        }
        "/model" | "/m" => {
            if parts.len() > 1 {
                if let Some(idx) = parts[1].find('/') {
                    let provider = &parts[1][..idx];
                    let model = &parts[1][idx + 1..];
                    if let Err(e) = agent_manager.switch_provider(provider, Some(model)).await {
                        push_log_line(logs, &format!("[Error] {}", e));
                    } else {
                        let status = agent_manager.status().await;
                        push_log_line(
                            logs,
                            &format!(
                                "[OK] Provider '{}' using model '{}'",
                                status.provider, status.model
                            ),
                        );
                    }
                } else if let Err(e) = agent_manager.switch_model(parts[1]).await {
                    push_log_line(logs, &format!("[Error] {}", e));
                }
            } else {
                let status = agent_manager.status().await;
                push_log_line(logs, &format!("Current model: {}", status.model));
            }
        }
        "/status" | "/st" => {
            let status = agent_manager.status().await;
            push_log_line(
                logs,
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
                push_log_line(logs, &format!("[Error] {}", e));
            } else {
                push_log_line(logs, "[OK] agent command executed");
            }
        }
        "/clear" | "/cls" => {
            logs.clear();
        }
        "/copy" => {
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let path = format!("/tmp/ndc-session-{}.txt", timestamp);
            match std::fs::write(&path, logs.join("\n")) {
                Ok(()) => push_log_line(logs, &format!("[OK] Session saved to: {}", path)),
                Err(e) => push_log_line(logs, &format!("[Error] Failed to save session: {}", e)),
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
                    push_log_line(logs, &format!("[OK] Session resumed: {}", sid));
                    restore_session_logs_to_panel(&agent_manager, viz_state, logs).await;
                }
                Err(e) => push_log_line(logs, &format!("[Error] {}", e)),
            }
        }
        "/new" => {
            match agent_manager.start_new_session().await {
                Ok(sid) => {
                    logs.clear();
                    push_log_line(logs, &format!("[OK] New session started: {}", sid));
                }
                Err(e) => push_log_line(logs, &format!("[Error] {}", e)),
            }
        }
        "/session" | "/sessions" => {
            let limit = parts
                .get(1)
                .and_then(|p| p.parse::<usize>().ok())
                .unwrap_or(10)
                .max(1);
            match agent_manager.list_project_session_ids(None, limit).await {
                Ok(ids) if ids.is_empty() => {
                    push_log_line(logs, "[Info] No sessions for current project.");
                }
                Ok(ids) => {
                    push_log_line(logs, "Sessions (newest first):");
                    for id in &ids {
                        push_log_line(logs, &format!("  {}", id));
                    }
                    push_log_line(
                        logs,
                        "Use /resume <id> to restore, or /resume for latest.",
                    );
                }
                Err(e) => push_log_line(logs, &format!("[Error] {}", e)),
            }
        }
        "/project" | "/projects" => {
            if parts.len() > 1 {
                // Switch to a project directory
                let dir = std::path::PathBuf::from(parts[1]);
                match agent_manager.switch_project_context(dir).await {
                    Ok(outcome) => {
                        logs.clear();
                        push_log_line(
                            logs,
                            &format!(
                                "[OK] Switched to project '{}' ({})",
                                outcome.project_id,
                                outcome.project_root.display()
                            ),
                        );
                        push_log_line(
                            logs,
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
                            push_log_line(
                                logs,
                                "Use /resume to restore session history into this panel.",
                            );
                        }
                    }
                    Err(e) => push_log_line(logs, &format!("[Error] {}", e)),
                }
            } else {
                // List known projects
                match agent_manager.discover_projects(10).await {
                    Ok(candidates) if candidates.is_empty() => {
                        push_log_line(logs, "[Info] No projects discovered.");
                    }
                    Ok(candidates) => {
                        push_log_line(logs, "Known projects:");
                        for c in &candidates {
                            push_log_line(
                                logs,
                                &format!(
                                    "  {} — {}",
                                    c.project_id,
                                    c.project_root.display()
                                ),
                            );
                        }
                        push_log_line(logs, "Use /project <dir> to switch.");
                    }
                    Err(e) => push_log_line(logs, &format!("[Error] {}", e)),
                }
            }
        }
        "/exit" => return Ok(true),
        _ => push_log_line(logs, "[Tip] Unknown command. Use /help."),
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
            ],
            || {
                let state = ReplVisualizationState::new(false);
                assert!(state.show_thinking);
                assert!(state.show_tool_details);
                assert!(state.expand_tool_cards);
                assert!(!state.live_events_enabled);
                assert_eq!(state.timeline_limit, 88);
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
        let event = ndc_core::AgentExecutionEvent {
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
        assert!(logs.iter().any(|l| l.contains("[Agent][r1] thinking...")));
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
        let mut logs = Vec::new();
        append_recent_thinking_to_logs(&mut logs, &viz);
        assert!(logs.iter().any(|l| l.contains("Recent Thinking")));
        assert!(logs.iter().any(|l| l.contains("plan: inspect files")));
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
        let mut logs = Vec::new();
        append_recent_timeline_to_logs(&mut logs, &viz);
        assert!(logs.iter().any(|l| l.contains("Recent Execution Timeline")));
        assert!(logs.iter().any(|l| l.contains("[stage:")));
        assert!(logs.iter().any(|l| l.contains("llm_round_1_start")));
        assert!(logs.iter().any(|l| l.contains("result_preview")));
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
        assert!(lines.iter().any(|line| line.contains("[Workflow][r2]")));
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
        assert!(lines.iter().any(|line| line.contains("[stage:verifying]")));
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
        assert!(lines.iter().any(|line| line.contains("[Usage][r3]")));
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
        let mut logs = Vec::new();
        append_runtime_metrics_to_logs(&mut logs, &viz);
        let joined = logs.join("\n");
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
        let mut logs = Vec::new();
        append_workflow_overview_to_logs(&mut logs, &viz, WorkflowOverviewMode::Verbose);
        let joined = logs.join("\n");
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
        let mut logs = Vec::new();
        append_workflow_overview_to_logs(&mut logs, &viz, WorkflowOverviewMode::Compact);
        let joined = logs.join("\n");
        assert!(joined.contains("Workflow Overview (compact)"));
        assert!(joined.contains("use /workflow verbose"));
        assert!(!joined.contains("r1"));
    }

    #[test]
    fn test_env_char_default_and_override() {
        with_env_overrides(&[("NDC_REPL_KEY_TOGGLE_THINKING", None)], || {
            assert_eq!(env_char("NDC_REPL_KEY_TOGGLE_THINKING", 't'), 't');
            unsafe { std::env::set_var("NDC_REPL_KEY_TOGGLE_THINKING", "X"); }
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
        let constraints = tui_layout_constraints();
        assert_eq!(
            constraints,
            [
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(4),
                Constraint::Length(4)
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
        let tool = style_session_log_line("[Tool][r1] failed read (3ms)");
        assert_eq!(line_plain(&tool), "[Tool][r1] failed read (3ms)");
        assert_eq!(tool.spans[0].style.fg, Some(Color::Red));

        let input = style_session_log_line("  ├─ input : {\"path\":\"README.md\"}");
        assert_eq!(line_plain(&input), "  ├─ input : {\"path\":\"README.md\"}");
        assert_eq!(input.spans[1].content.as_ref(), "input");
        assert_eq!(input.spans[1].style.fg, Some(Color::Cyan));

        let output = style_session_log_line("  ├─ output: ok");
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
            ],
            || {
                let mut viz = ReplVisualizationState::new(false);
                let mut logs = vec!["seed".to_string()];

                apply_tui_shortcut_action(TuiShortcutAction::ToggleThinking, &mut viz, &mut logs);
                assert!(viz.show_thinking);
                assert!(logs.iter().any(|l| l.contains("[OK] Thinking")));

                apply_tui_shortcut_action(TuiShortcutAction::ToggleDetails, &mut viz, &mut logs);
                assert!(viz.show_tool_details);
                assert!(logs.iter().any(|l| l.contains("[OK] Details")));

                apply_tui_shortcut_action(TuiShortcutAction::ToggleToolCards, &mut viz, &mut logs);
                assert!(viz.expand_tool_cards);
                assert!(logs.iter().any(|l| l.contains("[OK] Tool cards")));

                apply_tui_shortcut_action(TuiShortcutAction::ClearPanel, &mut viz, &mut logs);
                assert!(logs.is_empty());
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

        let mut logs = Vec::new();
        apply_tui_shortcut_action(TuiShortcutAction::ShowTimeline, &mut viz, &mut logs);

        assert!(logs.iter().any(|l| l.contains("Recent Execution Timeline")));
        assert!(logs.iter().any(|l| l.contains("llm_round_1_finish")));
    }

    #[test]
    fn test_runtime_shortcut_pipeline_ctrl_t_and_scroll_reset() {
        use crossterm::event::KeyEvent;
        with_env_overrides(&[("NDC_DISPLAY_THINKING", None)], || {
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
            let mut logs = (0..30).map(|i| format!("line-{}", i)).collect::<Vec<_>>();
            let before_scroll = calc_log_scroll(logs.len(), 10);
            assert!(before_scroll > 0);

            apply_tui_shortcut_action(action, &mut viz, &mut logs);
            assert!(viz.show_thinking);

            apply_tui_shortcut_action(TuiShortcutAction::ClearPanel, &mut viz, &mut logs);
            let after_scroll = calc_log_scroll(logs.len(), 10);
            assert_eq!(after_scroll, 0);
        });
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
        assert!(lines.iter().any(|l| l.contains("error")));
        assert!(lines.iter().any(|l| l.contains("denied")));
    }

    #[test]
    fn test_event_render_snapshot_collapsed_mode() {
        let mut viz = ReplVisualizationState::new(false);
        viz.show_tool_details = false;
        viz.expand_tool_cards = false;
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
            "[Tool][r1] start list".to_string(),
            "[Tool][r1] done list (2ms)".to_string(),
            "  ├─ output: Cargo.toml".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_event_render_snapshot_expanded_mode() {
        let mut viz = ReplVisualizationState::new(true);
        viz.show_tool_details = true;
        viz.expand_tool_cards = true;
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

        let mut collapsed = ReplVisualizationState::new(false);
        collapsed.show_tool_details = false;
        collapsed.expand_tool_cards = false;
        let collapsed_lines = render_event_snapshot(&events, &mut collapsed);
        let collapsed_expected = vec![
            "[Thinking][r1] (collapsed, use /t or /thinking show)".to_string(),
            "[Agent][r1] thinking...".to_string(),
            "[Tool][r1] done list (4ms)".to_string(),
            "  ├─ output: Cargo.toml".to_string(),
        ];
        assert_eq!(collapsed_lines, collapsed_expected);

        let mut details_only = ReplVisualizationState::new(true);
        details_only.show_tool_details = true;
        details_only.expand_tool_cards = false;
        let details_lines = render_event_snapshot(&events, &mut details_only);
        let details_expected = vec![
            "[Thinking][r1]".to_string(),
            "  └─ analyze project structure".to_string(),
            "[Step][r1] llm_round_1_start".to_string(),
            "[Tool][r1] done list (4ms)".to_string(),
            "  ├─ output: Cargo.toml".to_string(),
            "  └─ (collapsed card, use /cards or Ctrl+E)".to_string(),
        ];
        assert_eq!(details_lines, details_expected);

        let mut expanded = ReplVisualizationState::new(true);
        expanded.show_tool_details = true;
        expanded.expand_tool_cards = true;
        let expanded_lines = render_event_snapshot(&events, &mut expanded);
        let expanded_expected = vec![
            "[Thinking][r1]".to_string(),
            "  └─ analyze project structure".to_string(),
            "[Step][r1] llm_round_1_start".to_string(),
            "[Tool][r1] done list (4ms)".to_string(),
            "  ├─ output: Cargo.toml".to_string(),
            "  ├─ input : {\"path\":\".\"}".to_string(),
            "  └─ meta  : call_id=call-9 status=ok".to_string(),
        ];
        assert_eq!(expanded_lines, expanded_expected);

        let mut logs = Vec::new();
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
        append_recent_timeline_to_logs(&mut logs, &expanded);
        assert!(logs.iter().any(|l| l.contains("Recent Execution Timeline")));
        assert!(logs.iter().any(|l| l.contains("llm_round_1_finish")));
    }
}
