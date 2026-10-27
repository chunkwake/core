# Chunkwake

Многопоточное серверное ядро Minecraft Java Edition **26.3** на Rust, совместимое с плагинами Bukkit/Paper/Purpur.

- Архитектура: [ARCHITECTURE.md](ARCHITECTURE.md)
- Этапы: [docs/milestones.md](docs/milestones.md)
- Критерии оценки: [docs/criteria.md](docs/criteria.md)

Основная ветка: `ver/26.3`.

## Первый запуск

Нужен ванильный Minecraft 26.3 `server.jar`: Chunkwake читает из него номер протокола. Сейчас сервер реализует status и login, но ещё не переводит игрока в игровой мир. При `online-mode=true` вход проверяется через Minecraft session server; при `false` имена игроков не проверяются.

Соберите бинарник командой `cargo build --release -p chunkwake-server`. Он появится как `target/release/chunkwake` (Windows: `target\release\chunkwake.exe`). Создайте каталог сервера и положите туда `server.jar`. Затем в интерактивном терминале выполните команды ниже, заменив `chunkwake` на путь к бинарнику, если он не добавлен в `PATH`:

```text
chunkwake setup --dir /path/to/server
chunkwake doctor --dir /path/to/server
chunkwake --dir /path/to/server
```

В Windows PowerShell путь можно передать так: `chunkwake setup --dir "C:\Minecraft\Chunkwake"`. В Linux: `chunkwake setup --dir /srv/chunkwake`. `--dir` задаёт каталог для `server.properties` и относительного пути `vanilla-jar`; при запуске как службы передавайте его явно.

Мастер предлагает локальный адрес `127.0.0.1`. Для доступа из сети укажите в мастере `0.0.0.0` и подтвердите предупреждение: в текущем offline mode учётные записи игроков не проверяются.

Посмотреть и изменить настройки можно без редактора:

```text
chunkwake config show --dir /path/to/server
chunkwake config set server-port 25570 --dir /path/to/server
chunkwake doctor --dir /path/to/server
```

`setup` открывает форму в терминале: стрелки выбирают поле, Enter редактирует значение, Enter или Space переключает авторизацию, S сохраняет изменения, Esc выходит без сохранения. При повторном запуске мастер показывает текущие значения. Обычный запуск без `server.properties` завершится с подсказкой запустить мастер. Для справки используйте `chunkwake --help`.
