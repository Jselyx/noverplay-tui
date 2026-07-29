use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::credentials::{CredentialEditor, CredentialState};

pub(super) fn draw(
    frame: &mut Frame<'_>,
    editor: &CredentialEditor,
    credentials: CredentialState,
    area: Rect,
) {
    let popup = centered_rect(72, 12, area);
    frame.render_widget(Clear, popup);
    let configured = credentials.is_configured(editor.kind);
    let field = if editor.value.is_empty() {
        if configured {
            "новое значение не введено".to_string()
        } else {
            "вставь значение сюда".to_string()
        }
    } else {
        format!(
            "{}  {} символов",
            "•".repeat(editor.value.chars().count().min(48)),
            editor.value.chars().count()
        )
    };
    let hint = if editor.saving {
        "Сохраняем и перезапускаем провайдер"
    } else {
        "Enter сохранить  Esc отменить  Backspace стереть"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(editor.kind.hint()),
            Line::from(""),
            Line::from(format!("  {field}")),
            Line::from(""),
            Line::from(hint),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .style(Style::new().fg(Color::White).bg(Color::Black))
        .block(
            Block::new()
                .title(format!(" {} ", editor.kind.label()))
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::White)),
        ),
        popup,
    );
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let height = height.min(area.height);
    let vertical_margin = area.height.saturating_sub(height) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(vertical_margin),
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
        .split(vertical[1])[1]
}
