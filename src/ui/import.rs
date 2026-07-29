use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::importer::PlaylistImportEditor;

pub(super) fn draw(frame: &mut Frame<'_>, editor: &PlaylistImportEditor, area: Rect) {
    let popup = centered_rect(78, 12, area);
    frame.render_widget(Clear, popup);
    let value = if editor.url.is_empty() {
        "https://soundcloud.com/.../sets/... или https://music.yandex.ru/...".to_string()
    } else {
        editor.url.clone()
    };
    let hint = if editor.loading {
        "Читаем плейлист и убираем дубли"
    } else {
        "Enter импортировать  Esc отменить"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(value),
            Line::from(""),
            Line::from(hint),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .style(Style::new().fg(Color::White).bg(Color::Black))
        .block(
            Block::new()
                .title(" Импорт плейлиста ")
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::White)),
        ),
        popup,
    );
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let height = height.min(area.height);
    let margin = area.height.saturating_sub(height) / 2;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(margin),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(rows[1])[1]
}
