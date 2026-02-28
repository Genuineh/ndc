//! TUI application loop — the main ratatui event loop for the interactive session.

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};

use crate::agent_backend::{AgentBackend, TuiPermissionRequest};
use crate::layout_manager::tui_session_split;
use crate::todo_panel::render_todo_sidebar;

use super::*;

pub async fn run_repl_tui(
    viz_state: &mut ReplVisualizationState,
    agent_manager: std::sync::Arc<dyn AgentBackend>,
    mut permission_rx: tokio::sync::mpsc::Receiver<TuiPermissionRequest>,
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
        tokio::task::JoinHandle<anyhow::Result<ndc_core::AgentResponse>>,
    > = None;
    let mut streamed_count = 0usize;
    let mut streamed_any = false;
    let mut last_poll = Instant::now();
    let mut should_quit = false;
    let mut pending_permission_tx: Option<tokio::sync::oneshot::Sender<bool>> = None;
    let mut pending_permission_key: Option<String> = None;
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

        // Poll for incoming permission requests from the executor
        if let Ok(req) = permission_rx.try_recv() {
            viz_state.permission_blocked = true;
            viz_state.permission_pending_message = Some(req.description);
            pending_permission_key = req.permission_key;
            pending_permission_tx = Some(req.response_tx);
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
            let progress = build_workflow_progress_bar(viz_state, &theme);
            f.render_widget(Paragraph::new(progress), areas[1]);

            // [2] Conversation body (with optional TODO sidebar)
            let (conv_area, todo_area) =
                tui_session_split(areas[body_idx], viz_state.show_todo_panel);

            let body_block = Block::default()
                .title(Span::styled(
                    " Conversation ",
                    Style::default().fg(theme.primary),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_normal));
            let inner = body_block.inner(conv_area);
            session_view.body_height = (inner.height as usize).max(1);
            let styled_lines = style_chat_entries(entries.as_slice());
            let display_line_count = styled_lines.len();
            let scroll = effective_chat_scroll(&entries, &session_view) as u16;
            let body = Paragraph::new(Text::from(styled_lines))
                .block(body_block)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0));
            f.render_widget(body, conv_area);
            if display_line_count > session_view.body_height {
                let mut scrollbar_state = ScrollbarState::new(display_line_count)
                    .position(effective_chat_scroll(&entries, &session_view));
                let scrollbar = Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight)
                    .thumb_style(Style::default().fg(theme.text_muted));
                f.render_stateful_widget(scrollbar, conv_area, &mut scrollbar_state);
            }

            // Render TODO sidebar if visible
            if let Some(sidebar) = todo_area {
                render_todo_sidebar(
                    f,
                    sidebar,
                    &viz_state.todo_items,
                    viz_state.todo_scroll_offset,
                );
            }

            // [3] Permission bar (conditional)
            if has_permission {
                let perm_lines = build_permission_bar(viz_state, &theme);
                f.render_widget(Paragraph::new(Text::from(perm_lines)), areas[3]);
            }

            // [n-2] Status / hint bar
            let hint_line = build_status_hint_bar(
                input.as_str(),
                completion_state.as_ref(),
                viz_state,
                stream_state,
                &theme,
                is_processing,
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
                        if e.is_cancelled() {
                            // Task was aborted by Ctrl+C — already handled above
                        } else {
                            push_chat_entry(
                                &mut entries,
                                ChatEntry::ErrorNote(format!("[Error] join failed: {}", e)),
                            );
                        }
                    }
                }

                // Refresh TODO items after agent processing completes
                if let Ok(items) = agent_manager.list_session_todos().await {
                    viz_state.todo_items = items;
                }
            }
        }

        if event::poll(Duration::from_millis(20))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        if let Some(handle) = processing_handle.take() {
                            handle.abort();
                            live_events = None;
                            live_session_id = None;
                            push_chat_entry(
                                &mut entries,
                                ChatEntry::WarningNote(
                                    "[Interrupted] task cancelled by Ctrl+C".to_string(),
                                ),
                            );
                        } else {
                            should_quit = true;
                        }
                        continue;
                    }

                    if key.code == KeyCode::Esc {
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

                    // Handle y/n/a/s/p permission response keys when a permission prompt is active
                    if viz_state.permission_blocked {
                        if let Some(tx) = pending_permission_tx.take() {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let _ = tx.send(true);
                                    viz_state.permission_blocked = false;
                                    viz_state.permission_pending_message = None;
                                    pending_permission_key = None;
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') => {
                                    let _ = tx.send(false);
                                    viz_state.permission_blocked = false;
                                    viz_state.permission_pending_message = None;
                                    pending_permission_key = None;
                                }
                                KeyCode::Char('a') | KeyCode::Char('A') => {
                                    let _ = tx.send(true);
                                    viz_state.permission_blocked = false;
                                    viz_state.permission_pending_message = None;
                                    // Add to session-level security override for this permission
                                    if let Some(perm) = pending_permission_key.take() {
                                        let existing =
                                            std::env::var("NDC_SECURITY_OVERRIDE_PERMISSIONS")
                                                .unwrap_or_default();
                                        let new_val = if existing.is_empty() {
                                            perm
                                        } else if !existing.split(',').any(|s| s.trim() == perm) {
                                            format!("{},{}", existing, perm)
                                        } else {
                                            existing
                                        };
                                        // SAFETY: no other threads are reading this env var concurrently
                                        unsafe {
                                            std::env::set_var(
                                                "NDC_SECURITY_OVERRIDE_PERMISSIONS",
                                                new_val,
                                            )
                                        };
                                    } else {
                                        // No specific permission key — approve all for session
                                        unsafe { std::env::set_var("NDC_AUTO_APPROVE_TOOLS", "1") };
                                    }
                                }
                                KeyCode::Char('p') | KeyCode::Char('P') => {
                                    let _ = tx.send(true);
                                    viz_state.permission_blocked = false;
                                    viz_state.permission_pending_message = None;
                                    // Save permanently to config
                                    if let Some(perm) = pending_permission_key.take() {
                                        // Also add to session overrides
                                        let existing =
                                            std::env::var("NDC_SECURITY_OVERRIDE_PERMISSIONS")
                                                .unwrap_or_default();
                                        let new_val = if existing.is_empty() {
                                            perm.clone()
                                        } else if !existing.split(',').any(|s| s.trim() == perm) {
                                            format!("{},{}", existing, perm)
                                        } else {
                                            existing
                                        };
                                        unsafe {
                                            std::env::set_var(
                                                "NDC_SECURITY_OVERRIDE_PERMISSIONS",
                                                new_val,
                                            )
                                        };
                                        // Persist to config file
                                        if let Err(e) =
                                            ndc_core::NdcConfigLoader::save_approved_permission(
                                                &perm,
                                            )
                                        {
                                            tracing::warn!("Failed to persist permission: {}", e);
                                        }
                                    }
                                }
                                _ => {
                                    // Put the sender back — not consumed
                                    pending_permission_tx = Some(tx);
                                }
                            }
                        }
                        continue;
                    }

                    if processing_handle.is_some() {
                        // While processing, only Ctrl+C (handled above) is active
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
