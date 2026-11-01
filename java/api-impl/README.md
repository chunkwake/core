# api-impl

Реализация Bukkit/Paper API поверх Rust-ядра. Мост — Panama FFM (Java 22+), но сам API собирается под **Java 25**, как paper-api 26.x: классы в `chunkwake-api.jar` — class file 69, и на Java 21 плагины не запустятся. `doctor` проверяет версию, если в `plugins/` есть jar'ы.

## Сборка

```sh
java/gradlew -p java dist
```

Результат — `java/dist/`: `chunkwake-api.jar`, `libs/` (paper-api и его зависимости) и `plugins/chunkwake-test-plugin.jar`. Если JDK 25 нет, Gradle скачает его сам (foojay). Сборка Rust Gradle не вызывает.

Сервер ищет API так: сначала встроенный архив (распаковывается в `<каталог сервера>/api` при первом старте), затем `CHUNKWAKE_PLUGIN_API`, затем `<каталог сервера>/api`, затем `java/dist` (dev). В релизной сборке `java/dist` упакован в бинарь (`crates/bridge/build.rs`), поэтому рядом с `chunkwake.exe` ничего раскладывать не нужно. Сборка без встраивания — `CHUNKWAKE_API_BUNDLE=none`. Плагины — в `<каталог сервера>/plugins`. JVM — та же Java, что в `java-path` / `CHUNKWAKE_JAVA`; доп. опции — `CHUNKWAKE_JVM_ARGS`. Нет API или Java — сервер работает без плагинов, а `doctor` об этом скажет.

## Как устроен мост

- Rust (`crates/bridge`) грузит `jvm.dll`/`libjvm.so` и создаёт JVM (`JNI_CreateJavaVM`). JNI нужен только для одного вызова `Bridge.boot(tableAddress)`.
- Дальше всё идёт через таблицу слотов u64 (`Bridge.java` ↔ `crates/bridge/src/lib.rs`): Java кладёт туда адреса upcall-стабов (`enable`, `tick`, `join`, `quit`, `blockBreak`, `command`), Rust — адреса своих функций (`send_message`, `broadcast`, `log`, `set_commands`). Строки передаются как UTF-8 `ptr+len`.
- Флаги в общей памяти: «есть задачи планировщика» и маска слушателей. Rust читает их без вызова, поэтому без задач и без слушателей граница не пересекается.
- Все вызовы в Java идут из одного потока `plugins`. Это main thread Paper: события и синхронные задачи выполняются по порядку. Отменяемые события (`BlockBreakEvent`) Rust ждёт до 1 с.
- Огромные интерфейсы Bukkit (`Server`, `Player`, `Block`, `World`, `PluginManager`, `BukkitScheduler`) реализуются через `Stub`: прокси ищет метод с той же сигнатурой в классе-реализации. Чего нет, возвращает пустое значение своего типа и один раз пишет в журнал `api: <интерфейс>.<метод>` — список нехваток виден сразу. Строгий режим с исключением — `CHUNKWAKE_API_STRICT=1`.
- Плагины грузит свой загрузчик (`PluginLoader`, `ConfiguredPluginClassLoader`). Серверную часть Paper (`paperPluginManager`) мы не используем.

## Что работает (M4)

`plugin.yml` и `paper-plugin.yml` (только `main`/`name`), `onLoad`/`onEnable`, команды из `plugin.yml` (`/cmd` и дерево `commands` у клиента), `PlayerJoinEvent`/`PlayerQuitEvent` (сообщения входа/выхода от плагинов), `BlockBreakEvent` с отменой (блок возвращается клиенту), `sendMessage` (`String`/`Component`), `broadcastMessage`, `BukkitScheduler` (синхронные задачи по тикам, асинхронные в виртуальных потоках).

Ограничения: права не вычисляются (все игроки — операторы), мир один, у блока есть только тип и координаты, `onDisable` при остановке пока не вызывается.
