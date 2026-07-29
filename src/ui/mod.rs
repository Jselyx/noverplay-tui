mod credential;
mod onboarding;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};

use crate::{
    app::{App, Modal, PlaybackStatus, Screen},
    model::{RepeatMode, TrackRef},
};

const PRIMARY: Color = Color::White;
const MUTED: Color = Color::DarkGray;
const PANEL: Color = Color::Black;
const BORDER: Color = Color::White;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::new().fg(PRIMARY).bg(PANEL)), area);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(4)])
        .split(area);
    draw_main(frame, app, vertical[0]);
    draw_player(frame, app, vertical[1]);
    if let Some(modal) = &app.modal {
        draw_modal(frame, app, modal, area);
    }
}

fn draw_main(frame: &mut Frame<'_>, app: &App, area: Rect) {
    // узкий терминал тоже терминал, не заставляем его изображать телевизор 🫩✌️
    if area.width < 82 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(4)])
            .split(area);
        draw_tabs(frame, app, rows[0]);
        draw_screen(frame, app, rows[1]);
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(23), Constraint::Min(40)])
            .split(area);
        draw_sidebar(frame, app, columns[0]);
        draw_screen(frame, app, columns[1]);
    }
}

fn draw_tabs(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let titles = Screen::ALL
        .iter()
        .map(|screen| Line::from(format!("{} {}", screen.icon(), screen.label())))
        .collect::<Vec<_>>();
    let selected = Screen::ALL
        .iter()
        .position(|screen| *screen == app.screen)
        .unwrap_or_default();
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .style(Style::new().fg(MUTED).bg(PANEL))
            .highlight_style(Style::new().fg(Color::Black).bg(PRIMARY)),
        area,
    );
}

fn draw_sidebar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Screen::ALL
        .iter()
        .enumerate()
        .map(|(index, screen)| {
            let selected = *screen == app.screen;
            let style = if selected {
                Style::new()
                    .fg(Color::Black)
                    .bg(PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(MUTED)
            };
            ListItem::new(format!(
                " {}  {} {}",
                index + 1,
                screen.icon(),
                screen.label()
            ))
            .style(style)
        })
        .collect::<Vec<_>>();
    let block = Block::new()
        .title(Line::from(vec![
            Span::styled(" NOVER", Style::new().fg(Color::White)),
            Span::styled("PLAY ", Style::new().fg(PRIMARY)),
        ]))
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(BORDER))
        .style(Style::new().bg(PANEL));
    frame.render_widget(List::new(sections).block(block), area);
}

fn draw_screen(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);
    let title = if app.screen == Screen::Search {
        format!(" Поиск  / {}_ ", app.search_query)
    } else {
        format!(" {}  {} ", app.screen.icon(), app.screen.label())
    };
    frame.render_widget(
        Paragraph::new(title)
            .style(Style::new().fg(Color::White).add_modifier(Modifier::BOLD))
            .block(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::new().fg(BORDER)),
            ),
        rows[0],
    );
    match app.screen {
        Screen::Playlists => draw_playlists(frame, app, rows[1]),
        Screen::Profile => draw_profile(frame, rows[1]),
        Screen::Settings => draw_settings(frame, app, rows[1]),
        _ => draw_tracks(frame, app, rows[1]),
    }
    frame.render_widget(
        Paragraph::new(app.status_message.as_str())
            .alignment(Alignment::Right)
            .style(Style::new().fg(MUTED)),
        rows[2],
    );
}

fn draw_tracks(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let tracks = app.selected_tracks();
    // пустой список не баг, зато без скелетонов которые пукают вечной загрузкой АХАХАХА
    if tracks.is_empty() {
        let message = match app.screen {
            Screen::Search => "Напиши запрос и нажми Enter",
            Screen::Wave if app.wave_loading => "Собираем волну из истории и похожих треков",
            Screen::Wave => "Послушай или лайкни несколько треков и зайди сюда снова",
            Screen::Queue => "Очередь пуста",
            _ => "Здесь пока тихо",
        };
        frame.render_widget(
            Paragraph::new(format!("\n      ◢◤\n     ♫  ♫\n      ◥◣\n\n{message}"))
                .alignment(Alignment::Center)
                .style(Style::new().fg(MUTED))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    let items = tracks
        .iter()
        .enumerate()
        .map(track_item)
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(
        List::new(items).highlight_symbol(" ▸ ").highlight_style(
            Style::new()
                .fg(Color::Black)
                .bg(PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        area,
        &mut state,
    );
}

fn track_item((index, track): (usize, &TrackRef)) -> ListItem<'static> {
    let duration = track
        .duration_ms
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_string());
    let source = track.provider.label();
    ListItem::new(vec![
        Line::from(vec![
            Span::styled(format!(" {:02}  ", index + 1), Style::new().fg(MUTED)),
            Span::styled(track.title.clone(), Style::new().fg(Color::White)),
            Span::styled(format!("  {duration}"), Style::new().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::raw("     "),
            Span::styled(track.display_artist(), Style::new().fg(MUTED)),
            Span::styled(format!("  ·  {source}"), Style::new().fg(PRIMARY)),
        ]),
    ])
}

fn draw_playlists(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let items = app
        .playlists
        .iter()
        .map(|playlist| {
            ListItem::new(format!(
                "  ≡  {}  ·  {} треков",
                playlist.title,
                playlist.tracks.len()
            ))
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("Импортируй плейлист по ссылке, и он появится здесь")
                .alignment(Alignment::Center)
                .style(Style::new().fg(MUTED)),
            area,
        );
    } else {
        frame.render_widget(List::new(items), area);
    }
}

fn draw_profile(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::styled("             ●", Style::new().fg(PRIMARY)),
            Line::styled("         Гостевой режим", Style::new().fg(Color::White)),
            Line::from(""),
            Line::styled(
                "  Войди в Noverplay для серверного client_id",
                Style::new().fg(MUTED),
            ),
        ]),
        area,
    );
}

fn draw_settings(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let items = crate::credentials::CredentialKind::ALL
        .iter()
        .map(|kind| {
            let configured = app.credentials.is_configured(*kind);
            ListItem::new(format!(
                "  {:<24}  {}",
                kind.label(),
                if configured {
                    "настроен"
                } else {
                    "не настроен"
                }
            ))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::new().title(" Сервисы  Enter изменить "))
            .highlight_symbol(" ▸ ")
            .highlight_style(Style::new().fg(Color::Black).bg(Color::White)),
        area,
        &mut state,
    );
}

fn draw_player(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(24),
            Constraint::Length(25),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(" ◢██◣\n █♫ █\n ◥██◤").style(Style::new().fg(PRIMARY).bg(PANEL)),
        columns[0],
    );
    let track = app
        .now_playing
        .as_ref()
        .map(|track| format!("{}\n{}", track.title, track.display_artist()))
        .unwrap_or_else(|| "Ничего не играет\nВыбери трек".to_string());
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(columns[1]);
    frame.render_widget(
        Paragraph::new(track).style(Style::new().fg(Color::White).bg(PANEL)),
        center[0],
    );
    let ratio = if app.player.duration_ms == 0 {
        0.0
    } else {
        app.player.position_ms.min(app.player.duration_ms) as f64 / app.player.duration_ms as f64
    };
    frame.render_widget(
        Gauge::default()
            .ratio(ratio)
            .gauge_style(Style::new().fg(PRIMARY).bg(Color::DarkGray)),
        center[1],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{} / {}",
            format_duration(app.player.position_ms),
            format_duration(app.player.duration_ms)
        ))
        .style(Style::new().fg(MUTED).bg(PANEL)),
        center[2],
    );
    let status = match app.player.status {
        PlaybackStatus::Playing => "Ⅱ",
        PlaybackStatus::Buffering => "◌",
        _ => "▶",
    };
    frame.render_widget(
        Paragraph::new(format!(
            " {status}  p ◀  n ▶\n {}%  {}  {}\n Space пауза",
            app.player.volume_percent,
            if app.player.shuffle {
                "shuffle"
            } else {
                "linear"
            },
            repeat_label(app.player.repeat)
        ))
        .alignment(Alignment::Center)
        .style(Style::new().fg(Color::White).bg(PANEL)),
        columns[2],
    );
}

fn draw_modal(frame: &mut Frame<'_>, app: &App, modal: &Modal, area: Rect) {
    if let Modal::Onboarding(state) = modal {
        onboarding::draw(frame, state, area);
        return;
    }
    if let Modal::Credential(editor) = modal {
        credential::draw(frame, editor, app.credentials, area);
        return;
    }
    let popup = centered_rect(64, 60, area);
    frame.render_widget(Clear, popup);
    let (title, body) = match modal {
        Modal::Onboarding(_) => unreachable!(),
        Modal::Credential(_) => unreachable!(),
        Modal::Help => (
            " Клавиши ",
            "1-8 разделы    / поиск\n↑↓ или jk выбор   Enter открыть\nSpace пауза       n/p следующий/предыдущий\n←→ или hl ±10 сек  +/- громкость\ns перемешивание   r повтор\nq выход           Esc закрыть",
        ),
        Modal::CommandPalette => (
            " Команды ",
            "Поиск\nИмпорт плейлиста\nВойти в аккаунт\nНастроить сервисы\nПроверить SoundCloud",
        ),
    };
    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(Style::new().fg(Color::White).bg(PANEL))
            .block(
                Block::new()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(BORDER)),
            ),
        popup,
    );
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

fn format_duration(value: u64) -> String {
    let seconds = value / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

const fn repeat_label(value: RepeatMode) -> &'static str {
    match value {
        RepeatMode::Off => "repeat off",
        RepeatMode::All => "repeat all",
        RepeatMode::One => "repeat one",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{config::AppConfig, storage::Storage};

    fn app() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let app = App::load(
            &storage,
            &AppConfig {
                onboarding_completed: true,
                ..AppConfig::default()
            },
        )
        .unwrap();
        (temp, app)
    }

    #[test]
    fn wide_terminal_renders_sidebar_and_player() {
        let (_temp, app) = app();
        let backend = TestBackend::new(120, 32);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let content = terminal.backend().to_string();
        assert!(content.contains("NOVERPLAY"));
        assert!(content.contains("Ничего не играет"));
    }

    #[test]
    fn narrow_terminal_uses_tabs_without_panicking() {
        let (_temp, app) = app();
        let backend = TestBackend::new(54, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert!(terminal.backend().to_string().contains("Главная"));
    }

    #[test]
    fn onboarding_renders_real_steps_instead_of_one_dead_button() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.handle(crate::action::Action::ModalSubmit);
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let content = terminal.backend().to_string();
        assert!(content.contains("Как запускаемся"));
        assert!(content.contains("Гостевой режим"));
        assert!(content.contains("2/4"));
        assert!(terminal.backend().buffer().content().iter().all(|cell| {
            matches!(
                cell.fg,
                Color::Reset | Color::Black | Color::White | Color::Gray | Color::DarkGray
            ) && matches!(
                cell.bg,
                Color::Reset | Color::Black | Color::White | Color::Gray | Color::DarkGray
            )
        }));
    }

    #[test]
    fn blocked_soundcloud_screen_shows_all_three_choices() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        if let Some(Modal::Onboarding(state)) = app.modal.as_mut() {
            state.step = crate::onboarding::OnboardingStep::ZapretChoice;
            state.soundcloud_problem = Some("соединение закрыто".to_string());
        }
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let content = terminal.backend().to_string();
        assert!(content.contains("Автоматически добавить домены"));
        assert!(content.contains("Показать ручную инструкцию"));
        assert!(content.contains("Пропустить"));
    }

    #[test]
    fn interface_stays_monochrome() {
        let (_temp, app) = app();
        let backend = TestBackend::new(120, 32);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert!(terminal.backend().buffer().content().iter().all(|cell| {
            matches!(
                cell.fg,
                Color::Reset | Color::Black | Color::White | Color::Gray | Color::DarkGray
            ) && matches!(
                cell.bg,
                Color::Reset | Color::Black | Color::White | Color::Gray | Color::DarkGray
            )
        }));
    }

    #[test]
    fn credential_editor_masks_the_value() {
        let (_temp, mut app) = app();
        app.screen = Screen::Settings;
        app.modal = Some(Modal::Credential(Box::new(
            crate::credentials::CredentialEditor {
                kind: crate::credentials::CredentialKind::SoundCloudClientId,
                value: "super-secret-client-id".to_string(),
                saving: false,
            },
        )));
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let content = terminal.backend().to_string();
        assert!(content.contains("SoundCloud client_id"));
        assert!(!content.contains("super-secret-client-id"));
    }
}
