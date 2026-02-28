//! Command routing and display helpers for both legacy CLI and TUI modes.

use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use crate::agent_mode::{AgentModeManager, handle_agent_command};
use crate::redaction::{RedactionMode, sanitize_text};

use super::*;

// ===== Legacy CLI command handler =====

pub(crate) async fn handle_command(
    input: &str,
    _config: &crate::repl::ReplConfig,
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
            println!("[Tip] Unknown command. Use natural language or type '/help' for commands.");
        }
    }
}

// ===== Legacy agent dialogue handler =====

pub(crate) async fn handle_agent_dialogue(
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

// ===== Display helpers (legacy/CLI) =====

pub(crate) async fn show_agent_error(
    error: &ndc_core::AgentError,
    agent_manager: &Arc<AgentModeManager>,
) {
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

    let provider_upper = status.provider.to_uppercase();

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

pub(crate) fn show_help() {
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

// ===== TUI command routing =====

pub(crate) async fn restore_session_to_panel(
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

pub(crate) async fn handle_tui_command(
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

// ===== Legacy display functions =====

pub(crate) fn show_recent_thinking(
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

pub(crate) fn show_workflow_overview(
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

pub(crate) fn show_runtime_metrics(viz_state: &ReplVisualizationState) {
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

pub(crate) fn show_timeline(
    timeline: &[ndc_core::AgentExecutionEvent],
    limit: usize,
    mode: RedactionMode,
) {
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

pub(crate) async fn show_model_info(agent_manager: &AgentModeManager) {
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

pub(crate) fn show_agent_status(status: &crate::agent_mode::AgentModeStatus) {
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
