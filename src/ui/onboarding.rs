use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::onboarding::{OnboardingState, OnboardingStep, zapret::SOUNDCLOUD_DOMAINS};

const WHITE: Color = Color::White;
const MUTED: Color = Color::DarkGray;
const BLACK: Color = Color::Black;

pub(super) fn draw(frame: &mut Frame<'_>, state: &OnboardingState, area: Rect) {
    let popup = centered_rect(70, 72, area);
    frame.render_widget(Clear, popup);
    let body = popup.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    frame.render_widget(
        Block::new()
            .title(format!(" Быстрая настройка  {} ", progress(state.step)))
            .borders(Borders::ALL)
            .border_style(Style::new().fg(WHITE))
            .style(Style::new().fg(WHITE).bg(BLACK)),
        popup,
    );
    let lines = match state.step {
        OnboardingStep::Welcome => welcome(),
        OnboardingStep::Account => account(state),
        OnboardingStep::Providers => providers(state),
        OnboardingStep::Audio => audio(state),
        OnboardingStep::CheckingSoundCloud => checking(),
        OnboardingStep::ZapretChoice => zapret_choice(state),
        OnboardingStep::ZapretPath => zapret_path(state),
        OnboardingStep::ZapretReview => zapret_review(state),
        OnboardingStep::ZapretManual => zapret_manual(),
        OnboardingStep::Complete => vec![Line::from("Настройка завершена")],
    };
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false })
            .style(Style::new().fg(WHITE).bg(BLACK)),
        body,
    );
}

fn welcome() -> Vec<Line<'static>> {
    vec![
        heading("Noverplay готов поселиться в терминале"),
        Line::from(""),
        muted("Музыка и библиотека работают локально"),
        muted("Сервер нужен только аккаунту и ключу SoundCloud"),
        Line::from(""),
        Line::from("Enter  начать"),
        muted("Esc    пропустить и остаться гостем"),
    ]
}

fn account(state: &OnboardingState) -> Vec<Line<'static>> {
    vec![
        heading("Как запускаемся"),
        Line::from(""),
        option(0, state.selected, "Войти в аккаунт Noverplay"),
        option(1, state.selected, "Гостевой режим"),
        Line::from(""),
        muted("Аккаунт синхронизирует только профиль и выдаёт client_id"),
        muted("Гость использует собственный ключ SoundCloud"),
        Line::from(""),
        footer(),
    ]
}

fn providers(state: &OnboardingState) -> Vec<Line<'static>> {
    vec![
        heading("Музыкальные сервисы"),
        Line::from(""),
        toggle(0, state.selected, state.soundcloud_enabled, "SoundCloud"),
        toggle(1, state.selected, state.yandex_enabled, "Yandex Music"),
        option(2, state.selected, "Продолжить"),
        Line::from(""),
        muted("Deezer пока оставлен заделом без воспроизведения"),
        muted("Space или Enter переключает сервис"),
        Line::from(""),
        footer(),
    ]
}

fn audio(state: &OnboardingState) -> Vec<Line<'static>> {
    let mut lines = vec![heading("Аудиовыход"), Line::from("")];
    let start = state.selected.saturating_sub(4);
    lines.extend(
        state
            .audio_outputs
            .iter()
            .enumerate()
            .skip(start)
            .take(8)
            .map(|(index, output)| {
                let label = output.as_deref().unwrap_or("Системный аудиовыход");
                if index == state.selected {
                    selected_line(format!(" > {label} "))
                } else {
                    Line::styled(format!("   {label}"), Style::new().fg(MUTED))
                }
            }),
    );
    lines.push(Line::from(""));
    lines.push(muted(
        "↑↓ выбирает устройство, список не грузится целиком в экран",
    ));
    lines.push(Line::from(""));
    lines.push(Line::from("Enter  проверить SoundCloud и закончить"));
    lines
}

fn checking() -> Vec<Line<'static>> {
    vec![
        heading("Проверяем SoundCloud"),
        Line::from(""),
        Line::from("[ ··· ]  запрос к api-v2.soundcloud.com"),
        Line::from(""),
        muted("Если API отвечает, про Zapret здесь никто даже не заикнётся"),
    ]
}

fn zapret_choice(state: &OnboardingState) -> Vec<Line<'static>> {
    let reason = state
        .soundcloud_problem
        .clone()
        .unwrap_or_else(|| "SoundCloud недоступен".to_string());
    vec![
        heading("SoundCloud не прошёл проверку"),
        muted(reason),
        Line::from(""),
        option(0, state.selected, "Автоматически добавить домены"),
        option(1, state.selected, "Показать ручную инструкцию"),
        option(2, state.selected, "Пропустить"),
        Line::from(""),
        muted("Noverplay не скачивает Zapret, не повышает права и не трогает службу"),
        Line::from(""),
        footer(),
    ]
}

fn zapret_path(state: &OnboardingState) -> Vec<Line<'static>> {
    let mut lines = vec![
        heading("Путь к установленному Zapret"),
        Line::from(""),
        selected_line(format!(" {}_ ", state.zapret_path)),
        Line::from(""),
        muted("Windows: папка с bin/winws.exe"),
        muted("Linux: /opt/zapret"),
    ];
    if let Some(error) = &state.zapret_error {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            error.clone(),
            Style::new().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Enter  проверить путь и показать diff"));
    lines
}

fn zapret_review(state: &OnboardingState) -> Vec<Line<'static>> {
    let mut lines = vec![heading("Проверь изменения перед записью"), Line::from("")];
    if let Some(plan) = &state.zapret_plan {
        lines.extend(
            plan.render_diff()
                .lines()
                .map(|line| Line::from(line.to_string())),
        );
    }
    if let Some(error) = &state.zapret_error {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            error.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Enter  применить и создать backup"));
    lines.push(muted("Esc    не менять файлы"));
    lines
}

fn zapret_manual() -> Vec<Line<'static>> {
    vec![
        heading("Добавь домены вручную"),
        Line::from(""),
        Line::from(format!("  {}", SOUNDCLOUD_DOMAINS[0])),
        Line::from(format!("  {}", SOUNDCLOUD_DOMAINS[1])),
        Line::from(""),
        muted("Flowseal: lists/list-general-user.txt"),
        muted("Snowy-Fluffy: /opt/zapret/ipset/zapret-hosts-user.txt"),
        Line::from(""),
        muted("Snowy-Fluffy сейчас заморожен автором"),
        Line::from("После правки перезапусти Zapret самостоятельно"),
        Line::from(""),
        Line::from("Enter  закончить настройку"),
    ]
}

fn option(index: usize, selected: usize, label: &'static str) -> Line<'static> {
    if index == selected {
        selected_line(format!(" > {label} "))
    } else {
        Line::styled(format!("   {label}"), Style::new().fg(MUTED))
    }
}

fn toggle(index: usize, selected: usize, enabled: bool, label: &'static str) -> Line<'static> {
    let mark = if enabled { "x" } else { " " };
    let text = format!(" [{}] {label} ", mark);
    if index == selected {
        selected_line(format!(">{text}"))
    } else {
        Line::styled(format!(" {text}"), Style::new().fg(MUTED))
    }
}

fn selected_line(text: String) -> Line<'static> {
    Line::styled(
        text,
        Style::new()
            .fg(BLACK)
            .bg(WHITE)
            .add_modifier(Modifier::BOLD),
    )
}

fn heading(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::new().fg(WHITE).add_modifier(Modifier::BOLD),
    ))
}

fn muted(text: impl Into<String>) -> Line<'static> {
    Line::styled(text.into(), Style::new().fg(MUTED))
}

fn footer() -> Line<'static> {
    muted("↑↓ или jk выбрать   Enter продолжить   Esc пропустить")
}

fn progress(step: OnboardingStep) -> &'static str {
    match step {
        OnboardingStep::Welcome => "1/4",
        OnboardingStep::Account => "2/4",
        OnboardingStep::Providers => "3/4",
        OnboardingStep::Audio | OnboardingStep::CheckingSoundCloud => "4/4",
        OnboardingStep::ZapretChoice
        | OnboardingStep::ZapretPath
        | OnboardingStep::ZapretReview
        | OnboardingStep::ZapretManual => "доступ",
        OnboardingStep::Complete => "готово",
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
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
