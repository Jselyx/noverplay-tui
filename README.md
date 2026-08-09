# Noverplay TUI

<p align="center">
  <a href="https://github.com/Jselyx/noverplay-tui/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Jselyx/noverplay-tui/ci.yml?branch=main&amp;style=for-the-badge&amp;logo=githubactions&amp;logoColor=white&amp;label=CI" alt="CI"></a>
  <a href="#установка"><img src="https://img.shields.io/badge/Platforms-Windows%20%7C%20Linux-0078D4?style=for-the-badge&amp;logo=windowsterminal&amp;logoColor=white" alt="Windows and Linux"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPL--3.0--only-2EA44F?style=for-the-badge" alt="License: GPL-3.0-only"></a>
</p>

<p align="center">
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/Rust-2024-000000?style=for-the-badge&amp;logo=rust&amp;logoColor=white" alt="Rust 2024"></a>
  <a href="https://ratatui.rs/"><img src="https://img.shields.io/badge/TUI-Ratatui-F4B860?style=for-the-badge" alt="Ratatui"></a>
  <a href="https://tokio.rs/"><img src="https://img.shields.io/badge/Async-Tokio-2E5A88?style=for-the-badge" alt="Tokio"></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/Audio-CPAL%20%2B%20Symphonia-8A63D2?style=for-the-badge" alt="CPAL and Symphonia"></a>
  <a href="https://www.sqlite.org/"><img src="https://img.shields.io/badge/Storage-SQLite-07405E?style=for-the-badge&amp;logo=sqlite&amp;logoColor=white" alt="SQLite"></a>
</p>

**Noverplay TUI** — музыкальный клиент для терминала с единым поиском по SoundCloud, Yandex Music и Deezer, собственной очередью, историей прослушивания и локальным CLI для управления плеером.

[Возможности](#возможности) · [Установка](#установка) · [Быстрый старт](#быстрый-старт) · [Управление](#управление-в-tui) · [`np` CLI](#np--управление-из-командной-строки) · [Разработка](#разработка)

## Возможности

- единый поиск по всем настроенным музыкальным сервисам или по выбранной площадке;
- полноценное воспроизведение, пауза, перемотка, громкость, shuffle и три режима repeat;
- «Моя волна» на основе истории, любимых треков и рекомендаций провайдеров;
- локальная библиотека, постоянная очередь и история прослушивания;
- импорт плейлистов по ссылкам SoundCloud, Yandex Music и Deezer;
- гостевой режим и необязательный аккаунт Noverplay;
- настройка аудиовыхода, сервисов и глобальных хоткеев прямо в TUI;
- отдельная команда `np` для скриптов, хоткеев, панелей и других локальных интеграций — TUI держать открытым не нужно;
- адаптивный интерфейс для широких и узких терминалов.

Интерфейс состоит из восьми разделов: **Главная**, **Моя волна**, **Поиск**, **Библиотека**, **Плейлисты**, **Очередь**, **Профиль** и **Настройки**.

## Провайдеры

Noverplay подключает только включённые и настроенные сервисы. Если площадка недоступна или не настроена, остальные продолжают работать независимо.

| Сервис | Что нужно для подключения | Возможности |
| --- | --- | --- |
| SoundCloud | `client_id` или аккаунт Noverplay, который получает ключ автоматически | поиск, воспроизведение, плейлисты, похожие треки |
| Yandex Music | OAuth-токен | поиск, воспроизведение, плейлисты, персональные и похожие треки |
| Deezer | значение cookie `arl` | поиск, воспроизведение, плейлисты, похожие треки |

Ключи добавляются в разделе **Настройки**. Сначала Noverplay пытается сохранить их в системном хранилище учётных данных. Если оно недоступно, используется локальный файл `secrets.json`; на Unix для него выставляются права `0600`.

> Noverplay TUI не является официальным клиентом SoundCloud, Yandex Music или Deezer. Для работы провайдеров нужен действующий доступ к соответствующим сервисам.

## Установка

### Linux одной строкой

Последний релиз сразу с `noverplay` и `np`, без Rust, Cargo и прочего обряда посвящения:

```bash
curl -fsSL https://github.com/Jselyx/noverplay-tui/releases/latest/download/noverplay-linux-x86_64.tar.gz | sudo tar -xz -C /usr/local/bin noverplay np
```

Без `sudo` — в пользовательский каталог:

```bash
install -d "$HOME/.local/bin" && curl -fsSL https://github.com/Jselyx/noverplay-tui/releases/latest/download/noverplay-linux-x86_64.tar.gz | tar -xz -C "$HOME/.local/bin" noverplay np
```

Во втором случае убедитесь, что `$HOME/.local/bin` находится в `PATH`. Проверка:

```bash
noverplay --version
np --version
```

### Windows

Скачайте `noverplay-windows-x86_64.zip` из [последнего релиза](https://github.com/Jselyx/noverplay-tui/releases/latest), распакуйте архив и запустите `install.ps1`.

### Сборка из исходников

Cargo установит оба бинарника, `noverplay` и `np`.

### Требования

- актуальный стабильный [Rust toolchain](https://www.rust-lang.org/tools/install) с Cargo;
- Windows или Linux x86_64 — обе платформы проверяются в CI;
- на Debian/Ubuntu: `pkg-config` и заголовки ALSA.

```bash
# Только для Debian/Ubuntu
sudo apt install pkg-config libasound2-dev
```

### Сборка и установка

```bash
git clone https://github.com/Jselyx/noverplay-tui.git
cd noverplay-tui
cargo install --locked --path . --bins
```

Убедитесь, что каталог Cargo с бинарниками находится в `PATH`, затем проверьте установку:

```bash
noverplay --version
np --version
```

Если установка в `PATH` не нужна, проект можно просто собрать:

```bash
cargo build --locked --release
```

Готовые файлы появятся в `target/release/noverplay` и `target/release/np` с расширением `.exe` на Windows.

## Быстрый старт

Запустите клиент без аргументов:

```bash
noverplay
```

При первом запуске мастер предложит:

1. выбрать аккаунт Noverplay или гостевой режим;
2. включить нужные провайдеры;
3. выбрать аудиовыход;
4. проверить доступ к SoundCloud и при необходимости настроить Zapret.

После мастера откройте **Настройки**, добавьте ключи нужных сервисов и перейдите в **Поиск**. Нажмите `/`, введите запрос и подтвердите его клавишей `Enter`.

## Управление в TUI

Краткая справка открывается по `?`; полное окно сочетаний клавиш повторно открывается через `Ctrl+9`.

| Клавиши | Действие |
| --- | --- |
| `1` … `8` | перейти в один из восьми разделов |
| `/` | открыть поиск или вернуть фокус в строку поиска |
| `Tab` | переключить провайдера поиска |
| `Ctrl+J` | переключить фокус между запросом и результатами |
| `Alt+1` … `Alt+8` | сменить раздел, не выходя из режима ввода |
| `↑` / `↓`, `j` / `k` | выбрать элемент |
| `Enter` | открыть или запустить выбранный трек |
| `Space` | пауза / продолжить |
| `n` / `p` | следующий / предыдущий трек |
| `h` / `l`, `←` / `→` | перемотать на 10 секунд назад / вперёд |
| `+` / `-` | изменить громкость на 5% |
| `f` | добавить трек в библиотеку или убрать его оттуда |
| `s` | включить или выключить shuffle |
| `r` | переключить repeat: off → all → one |
| `i` | импортировать плейлист по ссылке |
| `Ctrl+K` | открыть палитру команд |
| `Esc` | вернуться назад или закрыть окно |
| `q` | выйти |

Глобальные хоткеи выключены по умолчанию и включаются в **Настройках**. Стандартные сочетания — `Ctrl+Alt+Space` для паузы, стрелки `Ctrl+Alt+←/→` для треков и `Ctrl+Alt+↑/↓` для громкости; каждое из них можно изменить.

## `np` — управление из командной строки

Второй бинарник, `np`, управляет плеером из shell-скриптов, внешних хоткеев и статус-баров. Если TUI закрыт, `np` сам запускает фоновый плеер:

```bash
np play massive attack
np status
```

Открывать `noverplay` заранее не нужно. История читается напрямую из локальной базы, а остальные команды используют запущенный TUI или автоматически поднимают фоновый процесс.

### Поиск и воспроизведение

```bash
np play massive attack
np play bjork @sc
np search radiohead --provider yandex
np wave
```

| Команда | Результат |
| --- | --- |
| `np play <запрос>` | находит и запускает первый доступный для воспроизведения трек |
| `np search <запрос>` | печатает найденные треки без запуска |
| `np wave` | собирает «Мою волну», заменяет ею очередь и запускает первый трек |

### Управление плеером

```bash
np pause
np resume
np toggle
np next
np previous
np stop
```

### Очередь

```bash
np queue list
np queue add portishead @dz
np queue remove 2
np queue clear
```

`queue remove` принимает видимую позицию, начиная с `1`. Очистка очереди также останавливает воспроизведение.

### Состояние и история

```bash
np status
np status --json

np history today
np history today --json
np history recent
np history recent 50
np history recent 50 --json
```

Без аргумента `np history recent` показывает последние 20 записей. Допустимый лимит — от 1 до 10 000.

При успешном вызове `np status --json` печатает объект состояния напрямую:

| Поле | Значение |
| --- | --- |
| `playback` | `playing`, `paused`, `buffering` или `stopped` |
| `track` | текущий трек или `null` |
| `position_ms` | текущая позиция в миллисекундах |
| `duration_ms` | длительность в миллисекундах |
| `volume_percent` | громкость от 0 до 100 |
| `queue_index` | индекс текущего трека с `0` или `null` |
| `queue_length` | количество треков в очереди |

Команды истории с `--json` возвращают массив записей с треком, временем `played_at_ms` и флагом `completed`.

### Выбор провайдера

По умолчанию запрос отправляется всем настроенным провайдерам. Площадку можно выбрать флагом или коротким тегом прямо в запросе:

| Провайдер | `--provider` | Теги в запросе |
| --- | --- | --- |
| Все настроенные | `all` | — |
| SoundCloud | `soundcloud`, `sc` | `@sc`, `#sc`, `@soundcloud`, `#soundcloud` |
| Yandex Music | `yandex`, `ya`, `ym` | `@ya`, `#ya`, `@ym`, `#ym`, `@yandex`, `#yandex` |
| Deezer | `deezer`, `dz` | `@dz`, `#dz`, `@deezer`, `#deezer` |

```bash
np play "teardrop" --provider soundcloud
np search kedr livanskiy @ya
np queue add moderation @deezer
```

Тег удаляется из поискового запроса. Конфликтующий `--provider` или несколько разных тегов завершают команду с понятной ошибкой.

`play`, `wave` и `queue add` выбирают только результат, для которого доступно воспроизведение. `play`, `search`, `wave` и `queue add` ждут финального ответа поиска и возвращают ошибку, если подходящего результата нет. Одновременно выполняется одна такая команда, но `status` и транспортные команды остаются доступны во время поиска.

## Настройка Zapret

Noverplay может добавить домены SoundCloud в уже установленный Zapret. Команда сначала показывает план изменений и просит подтверждение:

```bash
# Linux
noverplay setup-zapret --path /opt/zapret

# Windows — укажите реальный каталог своей установки
noverplay setup-zapret --path "C:\zapret"
```

Для неинтерактивного запуска используйте `--yes`:

```bash
noverplay setup-zapret --path /opt/zapret --yes
```

Перед изменением существующего списка создаётся резервная копия. После завершения Zapret нужно перезапустить вручную.

## Как устроено локальное управление

Владельцем проигрывателя, очереди и изменяемого состояния может быть интерактивный TUI или фоновый плеер. Активный процесс:

1. открывает control endpoint на случайном порту только в `127.0.0.1`;
2. генерирует случайный 256-битный токен;
3. сохраняет адрес, PID и токен в локальном каталоге данных;
4. удаляет endpoint при штатном завершении и очищает устаревший файл при следующем запуске.

`np` читает этот файл, проверяет loopback-адрес и отправляет одну JSON-команду владельцу состояния. Если владельца нет, `np` запускает `noverplay --background-player` и повторяет команду. При открытии TUI фоновый процесс передаёт ему управление. Control endpoint не публикуется в локальную сеть или интернет.

## Стек

- **Rust 2024**, Tokio и `async-trait` — приложение и асинхронная среда выполнения;
- **Ratatui** + Crossterm — терминальный интерфейс и события;
- **CPAL** + Symphonia — аудиовыход и декодирование MP3, AAC, FLAC, Ogg/Vorbis и WAV;
- **Reqwest** с Rustls — сетевые запросы без зависимости от системного OpenSSL;
- **SQLite** через `rusqlite` — библиотека, плейлисты, очередь и история;
- системный keyring с резервным локальным файлом — хранение токенов провайдеров.

## Разработка

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

CI выполняет форматирование, Clippy, тесты и release-сборку на Windows и Linux.

## Лицензия

Проект распространяется по лицензии [GNU GPL v3.0](LICENSE).
