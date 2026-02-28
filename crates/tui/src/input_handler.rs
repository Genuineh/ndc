//! Input Handler — input parsing, history, keymaps, slash command completion.
//!
//! Extracted from `repl.rs` (SEC-S1 God Object refactoring).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use super::{TUI_SCROLL_STEP, TuiSessionViewState, calc_log_scroll_usize, effective_log_scroll};

pub const AVAILABLE_PROVIDERS: &[&str] = &[
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
pub struct InputHistory {
    pub entries: Vec<String>,
    pub cursor: Option<usize>,
    pub draft: String,
    pub max_entries: usize,
}

impl InputHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            draft: String::new(),
            max_entries,
        }
    }

    pub fn push(&mut self, entry: String) {
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

    pub fn up(&mut self, current_input: &str) -> Option<&str> {
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

    pub fn down(&mut self) -> Option<&str> {
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

    pub fn reset(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }
}

#[derive(Debug, Clone)]
pub struct ReplTuiKeymap {
    pub toggle_thinking: char,
    pub toggle_details: char,
    pub toggle_tool_cards: char,
    pub show_recent_thinking: char,
    pub show_timeline: char,
    pub clear_panel: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiShortcutAction {
    ToggleThinking,
    ToggleDetails,
    ToggleToolCards,
    ShowRecentThinking,
    ShowTimeline,
    ClearPanel,
}

#[derive(Debug, Clone)]
pub struct ReplCommandCompletionState {
    pub suggestions: Vec<String>,
    pub selected_index: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct SlashCommandSpec {
    pub command: &'static str,
    pub _summary: &'static str,
}

pub const SLASH_COMMAND_SPECS: &[SlashCommandSpec] = &[
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
    pub fn from_env() -> Self {
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

pub fn env_bool(key: &str) -> Option<bool> {
    let value = std::env::var(key).ok()?.to_lowercase();
    match value.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse::<usize>().ok()
}

pub fn env_char(key: &str, default: char) -> char {
    std::env::var(key)
        .ok()
        .and_then(|v| v.chars().next())
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_alphanumeric())
        .unwrap_or(default)
}

pub fn key_is_ctrl_char(key: &KeyEvent, ch: char) -> bool {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    match key.code {
        KeyCode::Char(c) => c.eq_ignore_ascii_case(&ch),
        _ => false,
    }
}

pub fn detect_tui_shortcut(key: &KeyEvent, keymap: &ReplTuiKeymap) -> Option<TuiShortcutAction> {
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

pub fn canonical_slash_command(command: &str) -> &str {
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

pub fn slash_argument_options(command: &str) -> Option<&'static [&'static str]> {
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

pub fn parse_slash_tokens(input: &str) -> Option<(String, String, Vec<String>, bool)> {
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

pub fn matching_slash_commands(prefix: &str) -> Vec<SlashCommandSpec> {
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

pub fn completion_suggestions_for_input(input: &str) -> Vec<String> {
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
pub fn build_input_hint_lines(
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

pub fn apply_slash_completion(
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

pub fn move_session_scroll(session_view: &mut TuiSessionViewState, log_count: usize, delta: isize) {
    let max_scroll = calc_log_scroll_usize(log_count, session_view.body_height);
    let current = effective_log_scroll(log_count, session_view) as isize;
    let next = (current + delta).clamp(0, max_scroll as isize) as usize;
    session_view.scroll_offset = next;
    session_view.auto_follow = next >= max_scroll;
}

pub fn handle_session_scroll_key(
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

pub fn handle_session_scroll_mouse(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;
    use crossterm::event::{KeyCode, KeyModifiers, MouseEvent, MouseEventKind};

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
}
