//! TODO sidebar panel — renders a collapsible TODO list in the TUI.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::agent_backend::{TodoItem, TodoState};

/// State icon for each TODO state.
fn state_icon(state: &TodoState) -> &'static str {
    match state {
        TodoState::Pending => "☐",
        TodoState::InProgress => "▶",
        TodoState::Completed => "✓",
        TodoState::Failed => "✗",
        TodoState::Cancelled => "⊘",
    }
}

/// Color for each TODO state.
fn state_color(state: &TodoState) -> Color {
    match state {
        TodoState::Pending => Color::White,
        TodoState::InProgress => Color::Yellow,
        TodoState::Completed => Color::DarkGray,
        TodoState::Failed => Color::Red,
        TodoState::Cancelled => Color::DarkGray,
    }
}

/// Sort items: active first (Pending/InProgress), then completed/failed/cancelled.
fn sorted_items(items: &[TodoItem]) -> Vec<&TodoItem> {
    let mut sorted: Vec<&TodoItem> = items.iter().collect();
    sorted.sort_by_key(|item| match item.state {
        TodoState::InProgress => 0,
        TodoState::Pending => 1,
        TodoState::Failed => 2,
        TodoState::Completed => 3,
        TodoState::Cancelled => 4,
    });
    sorted
}

/// Render the TODO sidebar into the given area.
pub fn render_todo_sidebar(
    frame: &mut Frame,
    area: Rect,
    items: &[TodoItem],
    scroll_offset: usize,
) {
    let completed = items.iter().filter(|i| i.state == TodoState::Completed).count();
    let total = items.len();

    let title = format!(" TODO ({}/{}) ", completed, total);
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    let max_lines = inner.height as usize;
    let max_width = inner.width as usize;

    let sorted = sorted_items(items);

    let mut lines: Vec<Line<'_>> = Vec::new();
    let visible_items: Vec<&&TodoItem> = sorted.iter().skip(scroll_offset).collect();
    let remaining_after_view = if visible_items.len() > max_lines {
        visible_items.len() - max_lines
    } else {
        0
    };

    for item in visible_items.iter().take(if remaining_after_view > 0 {
        max_lines.saturating_sub(1)
    } else {
        max_lines
    }) {
        let icon = state_icon(&item.state);
        let color = state_color(&item.state);
        let prefix = format!("{} {}. ", icon, item.index);
        let title_width = max_width.saturating_sub(prefix.len());
        let display_title = if item.title.len() > title_width && title_width > 1 {
            let truncated: String = item.title.chars().take(title_width - 1).collect();
            format!("{}…", truncated)
        } else {
            item.title.clone()
        };

        let mut style = Style::default().fg(color);
        if item.state == TodoState::Completed || item.state == TodoState::Cancelled {
            style = style.add_modifier(Modifier::DIM);
        }

        lines.push(Line::from(vec![Span::styled(
            format!("{}{}", prefix, display_title),
            style,
        )]));
    }

    if remaining_after_view > 0 {
        lines.push(Line::from(Span::styled(
            format!("  ...还有 {} 项", remaining_after_view),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_items() -> Vec<TodoItem> {
        vec![
            TodoItem {
                id: "1".into(),
                index: 1,
                title: "Database migration".into(),
                state: TodoState::Completed,
            },
            TodoItem {
                id: "2".into(),
                index: 2,
                title: "Write tests".into(),
                state: TodoState::InProgress,
            },
            TodoItem {
                id: "3".into(),
                index: 3,
                title: "User auth".into(),
                state: TodoState::Pending,
            },
            TodoItem {
                id: "4".into(),
                index: 4,
                title: "API endpoints".into(),
                state: TodoState::Pending,
            },
        ]
    }

    #[test]
    fn test_state_icon_mapping() {
        assert_eq!(state_icon(&TodoState::Pending), "☐");
        assert_eq!(state_icon(&TodoState::InProgress), "▶");
        assert_eq!(state_icon(&TodoState::Completed), "✓");
        assert_eq!(state_icon(&TodoState::Failed), "✗");
        assert_eq!(state_icon(&TodoState::Cancelled), "⊘");
    }

    #[test]
    fn test_sorted_items_order() {
        let items = make_items();
        let sorted = sorted_items(&items);
        // InProgress first, then Pending, then Completed
        assert_eq!(sorted[0].state, TodoState::InProgress);
        assert_eq!(sorted[1].state, TodoState::Pending);
        assert_eq!(sorted[2].state, TodoState::Pending);
        assert_eq!(sorted[3].state, TodoState::Completed);
    }

    #[test]
    fn test_state_colors() {
        assert_eq!(state_color(&TodoState::Pending), Color::White);
        assert_eq!(state_color(&TodoState::InProgress), Color::Yellow);
        assert_eq!(state_color(&TodoState::Completed), Color::DarkGray);
        assert_eq!(state_color(&TodoState::Failed), Color::Red);
        assert_eq!(state_color(&TodoState::Cancelled), Color::DarkGray);
    }
}
