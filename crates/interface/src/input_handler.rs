//! Input Handler — input parsing, history, keymaps, slash command completion.
//!
//! Extracted from `repl.rs` (SEC-S1 God Object refactoring).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use super::{TUI_SCROLL_STEP, TuiSessionViewState, calc_log_scroll_usize, effective_log_scroll};

pub(crate) const AVAILABLE_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "minimax",
    "minimax-coding-plan",
    "minimax-cn",
    "minimax-cn-coding-plan",
    "openrouter",
    "ollama",
];

#[derive(Debug, Clone)]
pub(crate) struct InputHistory {
    pub(crate) entries: Vec<String>,
    pub(crate) cursor: Option<usize>,
    pub(crate) draft: String,
    pub(crate) max_entries: usize,
}

impl InputHistory {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            draft: String::new(),
            max_entries,
        }
    }

    pub(crate) fn push(&mut self, entry: String) {
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

    pub(crate) fn up(&mut self, current_input: &str) -> Option<&str> {
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

    pub(crate) fn down(&mut self) -> Option<&str> {
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

    pub(crate) fn reset(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReplTuiKeymap {
    pub(crate) toggle_thinking: char,
    pub(crate) toggle_details: char,
    pub(crate) toggle_tool_cards: char,
    pub(crate) show_recent_thinking: char,
    pub(crate) show_timeline: char,
    pub(crate) clear_panel: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiShortcutAction {
    ToggleThinking,
    ToggleDetails,
    ToggleToolCards,
    ShowRecentThinking,
    ShowTimeline,
    ClearPanel,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplCommandCompletionState {
    pub(crate) suggestions: Vec<String>,
    pub(crate) selected_index: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SlashCommandSpec {
    pub(crate) command: &'static str,
    pub(crate) _summary: &'static str,
}

pub(crate) const SLASH_COMMAND_SPECS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        command: "/help",
        _summary: "show help",
    },
    SlashCommandSpec {
        command: "/provider",
        _summary: "switch provider",
    },
    SlashCommandSpec {
        command: "/providers",
        _summary: "alias of /provider",
    },
    SlashCommandSpec {
        command: "/model",
        _summary: "switch model",
    },
    SlashCommandSpec {
        command: "/agent",
        _summary: "agent controls",
    },
    SlashCommandSpec {
        command: "/status",
        _summary: "show status",
    },
    SlashCommandSpec {
        command: "/thinking",
        _summary: "toggle thinking",
    },
    SlashCommandSpec {
        command: "/details",
        _summary: "toggle details",
    },
    SlashCommandSpec {
        command: "/cards",
        _summary: "toggle tool cards",
    },
    SlashCommandSpec {
        command: "/verbosity",
        _summary: "compact/normal/verbose",
    },
    SlashCommandSpec {
        command: "/stream",
        _summary: "stream on/off/status",
    },
    SlashCommandSpec {
        command: "/workflow",
        _summary: "workflow overview",
    },
    SlashCommandSpec {
        command: "/tokens",
        _summary: "usage metrics",
    },
    SlashCommandSpec {
        command: "/metrics",
        _summary: "runtime metrics",
    },
    SlashCommandSpec {
        command: "/timeline",
        _summary: "show timeline",
    },
    SlashCommandSpec {
        command: "/clear",
        _summary: "clear panel",
    },
    SlashCommandSpec {
        command: "/copy",
        _summary: "save session to file",
    },
    SlashCommandSpec {
        command: "/resume",
        _summary: "resume latest session",
    },
    SlashCommandSpec {
        command: "/new",
        _summary: "start new session",
    },
    SlashCommandSpec {
        command: "/session",
        _summary: "list sessions for current project",
    },
    SlashCommandSpec {
        command: "/project",
        _summary: "list or switch project",
    },
    SlashCommandSpec {
        command: "/exit",
        _summary: "exit repl",
    },
];

impl ReplTuiKeymap {
    pub(crate) fn from_env() -> Self {
        Self {
            toggle_thinking: env_char("NDC_REPL_KEY_TOGGLE_THINKING", 't'),
            toggle_details: env_char("NDC_REPL_KEY_TOGGLE_DETAILS", 'd'),
            toggle_tool_cards: env_char("NDC_REPL_KEY_TOGGLE_TOOL_CARDS", 'e'),
            show_recent_thinking: env_char("NDC_REPL_KEY_SHOW_RECENT_THINKING", 'y'),
            show_timeline: env_char("NDC_REPL_KEY_SHOW_TIMELINE", 'i'),
            clear_panel: env_char("NDC_REPL_KEY_CLEAR_PANEL", 'l'),
        }
    }
}

pub(crate) fn env_bool(key: &str) -> Option<bool> {
    let value = std::env::var(key).ok()?.to_lowercase();
    match value.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub(crate) fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse::<usize>().ok()
}

pub(crate) fn env_char(key: &str, default: char) -> char {
    std::env::var(key)
        .ok()
        .and_then(|v| v.chars().next())
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_alphanumeric())
        .unwrap_or(default)
}

pub(crate) fn key_is_ctrl_char(key: &KeyEvent, ch: char) -> bool {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    match key.code {
        KeyCode::Char(c) => c.eq_ignore_ascii_case(&ch),
        _ => false,
    }
}

pub(crate) fn detect_tui_shortcut(
    key: &KeyEvent,
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

pub(crate) fn canonical_slash_command(command: &str) -> &str {
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

pub(crate) fn slash_argument_options(command: &str) -> Option<&'static [&'static str]> {
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

pub(crate) fn parse_slash_tokens(input: &str) -> Option<(String, String, Vec<String>, bool)> {
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

pub(crate) fn matching_slash_commands(prefix: &str) -> Vec<SlashCommandSpec> {
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

pub(crate) fn completion_suggestions_for_input(input: &str) -> Vec<String> {
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
pub(crate) fn build_input_hint_lines(
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
            .map(|spec| format!("{} ({})", spec.command, spec._summary))
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
            .map(|spec| spec._summary)
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

pub(crate) fn apply_slash_completion(
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

pub(crate) fn move_session_scroll(
    session_view: &mut TuiSessionViewState,
    log_count: usize,
    delta: isize,
) {
    let max_scroll = calc_log_scroll_usize(log_count, session_view.body_height);
    let current = effective_log_scroll(log_count, session_view) as isize;
    let next = (current + delta).clamp(0, max_scroll as isize) as usize;
    session_view.scroll_offset = next;
    session_view.auto_follow = next >= max_scroll;
}

pub(crate) fn handle_session_scroll_key(
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

pub(crate) fn handle_session_scroll_mouse(
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
