# Noverplay TUI

Терминальный музыкальный клиент. Запуск без аргументов по-прежнему открывает TUI:

```sh
noverplay
```

`noverplay setup-zapret` сохраняет прежнее поведение настройки Zapret.

## `np` — управление запущенным TUI

Вместе с `noverplay` собирается отдельный бинарник `np`. TUI владеет проигрывателем и очередью и публикует локальный JSON endpoint только на `127.0.0.1`; endpoint защищён случайным токеном и удаляется при завершении.

```sh
np play massive attack
np play bjork @sc
np search radiohead --provider yandex
np wave
np pause
np resume
np toggle
np next
np previous
np stop

np queue list
np queue add portishead @dz
np queue remove 2
np queue clear

np status
np status --json
np history today
np history recent 50
np history recent 50 --json
```

`np status --json` печатает объект состояния напрямую. `queue_index` в нём нулевой (или `null`, когда текущего трека нет), остальные основные поля: `playback`, `track`, `position_ms`, `duration_ms`, `volume_percent`, `queue_length`.

По умолчанию поиск идёт по всем настроенным провайдерам. Поддерживаются `--provider all|soundcloud|yandex|deezer` (также короткие `sc|ya|ym|dz`) и теги `@sc`, `@ya`, `@dz`; варианты с `#` и полными именами тоже распознаются. `play`, `wave` и `queue add` выбирают первый результат с доступным воспроизведением. Поиск и изменения очереди выполняются владельцем состояния — запущенным TUI. `play`, `search`, `wave` и `queue add` ждут итог поиска и возвращают ошибку, если доступный трек не найден; транспортные команды и `status` остаются доступны во время поиска. Если TUI отсутствует, `np` завершается с явной ошибкой.

## Проверка

```sh
cargo fmt --check
cargo check --all-targets
cargo test
```
