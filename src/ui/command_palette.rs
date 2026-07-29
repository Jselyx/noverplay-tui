use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use crate::command_palette::{CommandPalette, PaletteCommand};

pub(super) fn draw(frame: &mut Frame<'_>, state: &CommandPalette, area: Rect) {
    let popup = centered(area);
    frame.render_widget(Clear, popup);
    let items = PaletteCommand::ALL
        .iter()
        .map(|command| ListItem::new(format!("  {}", command.label())))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default().with_selected(Some(state.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::new()
                    .title(" Команды  Enter запустить ")
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(Color::White)),
            )
            .style(Style::new().fg(Color::DarkGray).bg(Color::Black))
            .highlight_symbol(" ▸ ")
            .highlight_style(Style::new().fg(Color::Black).bg(Color::White)),
        popup,
        &mut list_state,
    );
}

fn centered(area: Rect) -> Rect {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(9),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(rows[1])[1]
}
