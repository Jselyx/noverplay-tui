use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::account::{
    captcha::{CellArea, account_popup_area, captcha_cell_area},
    models::AccountAction,
    state::{AccountDialog, AccountDialogStage},
};

pub(super) fn draw(frame: &mut Frame<'_>, dialog: &AccountDialog, area: Rect) {
    let popup = absolute_rect(account_popup_area(area.width, area.height), area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::new()
            .title(match dialog.action {
                AccountAction::Login => " Вход в Noverplay ",
                AccountAction::Register => " Регистрация в Noverplay ",
            })
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::White))
            .style(Style::new().fg(Color::White).bg(Color::Black)),
        popup,
    );

    match dialog.stage {
        AccountDialogStage::Credentials => draw_credentials(frame, dialog, popup),
        AccountDialogStage::LoadingCaptcha => draw_wait(frame, "Загружаем CAPTCHA", popup),
        AccountDialogStage::Captcha => draw_captcha(frame, dialog, area, popup),
        AccountDialogStage::Submitting => draw_wait(
            frame,
            match dialog.action {
                AccountAction::Login => "Проверяем вход",
                AccountAction::Register => "Создаём аккаунт",
            },
            popup,
        ),
    }
}

fn draw_credentials(frame: &mut Frame<'_>, dialog: &AccountDialog, popup: Rect) {
    let password = "•".repeat(dialog.password.chars().count());
    let username_style = field_style(dialog.selected_field == 0);
    let password_style = field_style(dialog.selected_field == 1);
    let mode = match dialog.action {
        AccountAction::Login => "Вход",
        AccountAction::Register => "Регистрация",
    };
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" F2 ", Style::new().fg(Color::Black).bg(Color::White)),
            Span::raw(format!("  режим: {mode}")),
        ]),
        Line::from(""),
        Line::styled(" Логин", Style::new().fg(Color::DarkGray)),
        Line::styled(format!(" {} ", dialog.username), username_style),
        Line::from(""),
        Line::styled(" Пароль", Style::new().fg(Color::DarkGray)),
        Line::styled(format!(" {password} "), password_style),
        Line::from(""),
    ];
    if let Some(error) = &dialog.error {
        lines.push(Line::styled(
            format!(" {error}"),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
    }
    lines.extend([
        Line::from(""),
        Line::styled(
            " Tab поле   Enter продолжить   F2 режим   Esc отменить",
            Style::new().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::new().fg(Color::White).bg(Color::Black))
            .wrap(Wrap { trim: false }),
        inner(popup),
    );
}

fn draw_captcha(frame: &mut Frame<'_>, dialog: &AccountDialog, area: Rect, popup: Rect) {
    let captcha_area = absolute_rect(captcha_cell_area(area.width, area.height), area);
    let text_mode = dialog
        .challenge
        .as_ref()
        .is_some_and(|challenge| challenge.captcha_kind == "text");
    frame.render_widget(
        Paragraph::new(if text_mode {
            "Реши задачу"
        } else {
            "Нажми иконки по порядку"
        })
        .alignment(Alignment::Center)
        .style(Style::new().fg(Color::White).bg(Color::Black)),
        Rect::new(
            popup.x.saturating_add(1),
            popup.y.saturating_add(1),
            popup.width.saturating_sub(2),
            1,
        ),
    );
    if let Some(raster) = &dialog.raster {
        let lines = raster
            .lines(captcha_area.width, captcha_area.height)
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines).style(Style::new().fg(Color::White).bg(Color::Black)),
            captcha_area,
        );
    } else if text_mode {
        let prompt = dialog
            .challenge
            .as_ref()
            .and_then(|challenge| challenge.prompt.as_deref())
            .unwrap_or("Введи ответ на вопрос сервера");
        frame.render_widget(
            Paragraph::new(format!("\n\n{prompt}"))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .style(Style::new().fg(Color::White).bg(Color::Black)),
            captcha_area,
        );
    }
    let progress = if text_mode {
        format!("Ответ: {}_", dialog.captcha_answer)
    } else {
        format!(
            "Выбрано {}/{}",
            dialog.selected_clicks(),
            dialog.required_clicks()
        )
    };
    let mut footer = vec![Line::styled(progress, Style::new().fg(Color::White))];
    if let Some(error) = &dialog.error {
        footer.push(Line::styled(
            error.clone(),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
    }
    footer.push(Line::styled(
        if text_mode {
            "Backspace стереть   Enter отправить   Esc отменить"
        } else {
            "Backspace отменить точку   Enter отправить   Esc отменить"
        },
        Style::new().fg(Color::DarkGray),
    ));
    let footer_height = footer.len().min(3) as u16;
    frame.render_widget(
        Paragraph::new(footer)
            .alignment(Alignment::Center)
            .style(Style::new().bg(Color::Black)),
        Rect::new(
            popup.x.saturating_add(1),
            popup
                .y
                .saturating_add(popup.height.saturating_sub(footer_height + 1)),
            popup.width.saturating_sub(2),
            footer_height,
        ),
    );
}

fn draw_wait(frame: &mut Frame<'_>, message: &str, popup: Rect) {
    frame.render_widget(
        Paragraph::new(format!("\n\n{message}"))
            .alignment(Alignment::Center)
            .style(Style::new().fg(Color::White).bg(Color::Black)),
        inner(popup),
    );
}

fn absolute_rect(area: CellArea, parent: Rect) -> Rect {
    Rect::new(
        parent.x.saturating_add(area.x),
        parent.y.saturating_add(area.y),
        area.width,
        area.height,
    )
}

fn inner(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

fn field_style(selected: bool) -> Style {
    if selected {
        Style::new()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::White).bg(Color::Black)
    }
}
