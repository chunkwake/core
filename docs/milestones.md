# Этапы

- **M0 — фундамент.** `git init`, ветка `ver/26.3`, workspace, CI (fmt/clippy/test, Windows+Linux), лицензия (проверить: ванильные данные не коммитить, только генерировать), `ARCHITECTURE.md`, `docs/criteria.md` (см. ниже).
- **M1 — протокол.** Codegen пакетов 26.3, status ping (сервер виден в списке), login offline/online (Mojang auth), configuration-фаза с реестрами.
- **M2 — пустой мир.** Вход в void/flat мир, движение, чанк-стриминг, keep-alive, чат, таб-лист. Бенчмарк: 1000 ботов (mc bot-клиент/`azalea`).
- **M3 — мир и хранение.** Anvil load/save, палитры, свет, ставка/ломка блоков, async I/O, region-тикинг на 1 мире.
- **M4 — JVM bridge MVP.** Загрузка `plugin.yml`/`paper-plugin.yml`, `onEnable`, команды, `PlayerJoinEvent`, `BlockBreakEvent`, `sendMessage`, scheduler. Тест-плагин в репо.
- **M5 — сущности и геймплей ч.1.** Игроки, инвентари, предметы, мобы базовые, урон, смерть/респаун.
- **M6 — расширение API.** Покрытие Bukkit API по приоритету популярных плагинов: LuckPerms, Vault, EssentialsX, WorldEdit/FAWE, WorldGuard, PlaceholderAPI, ProtocolLib/PacketEvents (отдельный слой эмуляции packet API).
- **M7 — генерация мира, редстоун, остальная ванила** (итеративно, по фич-матрице).
- **M8 — производительность и стабильность.** Профилирование, нагрузочные тесты, fuzzing протокола, длительные soak-тесты.
- **M9 — публичная альфа.**

- **M4a** — FFM-мост + off-heap views (вместо простого JNI MVP).
- **M4b** — STM-тик: read/write-set, детерминированный коммит, откат; тест-сьют гонок на синтетических плагинах.
- **M6a** — компилятор совместимости (ASM-анализ + shim'ы).
- **M3a** — event-sourced лог изменений + WAL (закладывается одновременно с миром, т.к. на нём строятся сеть и хранение).

