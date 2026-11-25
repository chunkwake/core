# Восстановленный plugin gate (25.11.2026)

Исходный стенд `load-gate-extra` был удалён вместе с диском C:. Стенд собран
заново из Modrinth-плагинов для Paper/Bukkit/Spigot 26.3 и локальных
test/probe-плагинов.

## Результат

```
plugins: enabled 100 plugin(s) in 68896 ms
FAILED: (пусто)
LOADERR: (пусто)
SKIPPED: (пусто)
PANICS: 0
JARS: 104
PROBES: folia=True cw-folia=True
PROBE-FAIL: (пусто)
```

Контрольный прогон на финальном бинаре с dynamic worlds, player effects и
mob species — 25.11.2026 22:35, 100/100. Время плавает от сетевых проверок
плагинов (Geyser и др.): соседний прогон в 21:50 занял 136521 ms с тем же
результатом 100/100.

`JARS: 104` — 100 плагинов + библиотечные/зависимые jar-ы, которые Chunkwake
не считает отдельными плагинами. Финальный прогон 25.11.2026: 100/100, без failed/loaderr/skipped/panics. Гейт теперь **дожидается выполнения probe-задач** после enable: `FoliaProbe` должен пройти все 7 планировщиков (`all-schedulers-ok`), `ChunkwakeTest` — `folia: done global-fixed-rate (4/4)`. Если маркер не появился за 120 с, скрипт печатает `PROBE-FAIL` и возвращает exit 1. Раньше сервер убивался сразу после `enabled`, и FoliaProbe успевал только до `4/7` — часть Folia-планировщиков вообще не проверялась. Для GravesX в тестовом стенде отключён update-check (он зависает на сетевой загрузке библиотек). Добавлены ещё 4 тяжёлых NMS/API-плагина: `Skript`, `ProtocolLib`, `Orebfuscator`, `AxiomPaper`. Ранее были добавлены 6 реальных Paper/Folia-плагинов: `mob-heads`, `BuildPaste`, `no-enderman-grief`, `instantrestock`, `legacyfreecam`, `EnderChest`. CommandAPI и Pl3xMap больше не пропускаются:
добавлены `WorldData`, `ServerFunctionLibrary`, `LevelData`, `getSeed`,
`StreamMemberEncoder` и `CustomPacketPayload.codec`. Конфликт WorldEdit/FAWE
закрыт порядком зависимостей в `PluginClassGroup`: точное имя плагина
важнее `provides`-алиаса.

Полный список файлов:
[gate-restored-plugins.txt](gate-restored-plugins.txt).

## Важно

- Это **не побайтно тот же** набор, что старый 83/83: старые jar-ы утеряны.
  Набор собран заново из доступных 26.3-версий; часть проектов заменилась
  более новыми сборками.
- `journeymap` (клиентский мод), `QuickShop-Hikari` (конфликт WorldEdit),
  `EconomyShopGUI`, `ClickVillagers`, `FreedomChat`, `NexusCore`,
  `NexusAuctionHouse`, `DonutAuctionHouse`, `BKCommonLib`, `Sit`,
  `CrazyEnchantments` и `LagFixer` были исключены: они падали на этом стенде.
- Добавлены реальные Folia/Leaf-совместимые плагины: `Chunksmith`,
  `OneBlock`, `SmartSpawner`, `ClearLag++`, `Pl3xMap`, `AdvancedPortals`,
  `PvPManager`.
- CE включается; data pack загружает 45 recipes и 40 blocks,
  `CEProbe`/`CEStateProbe` проходят custom blocks, а `BlockState.blockState()`
  и entity world tracking hook больше не падают. Этот gate проверяет
  загрузку/enable плагинов, выполнение Folia/async probe-задач и отсутствие
  runtime-ошибок; полная parity произвольных плагинов им не доказывается.
- Отдельный CraftEngine parity-прогон: `CEProbe`/`CEStateProbe` на Paper и
  Chunkwake, 28/28 ключевых наблюдений совпали (BlockData, state, collision,
  drops, persist/fresh для `topaz_ore`, `table_lamp`, `palm_stairs`,
  `palm_door`). См. `docs/ce-parity.md` и `benchmarks/ce-parity/`.
- Console-command parity: 10 команд WorldEdit, Multiverse-Core, PlaceholderAPI,
  Chunky, BlueMap, ViaVersion, Plan, Skript, LuckPerms, TAB — 10/10 распознаны
  и вернули ожидаемый ответ на Paper и Chunkwake. Попутно исправлен
  `CommandSender.spigot()` у консоли (`Console.spigot()` теперь пересылает
  `BaseComponent` в лог, BlueMap больше не падает с NPE). См.
  `docs/plugin-commands.md` и `benchmarks/plugin-commands/`.
- Базовый `load-gate-server` (27/27) этим прогоном не восстанавливался.

## Воспроизведение

```powershell
powershell -ExecutionPolicy Bypass -File scripts/restore_gate_plugins.ps1
powershell -ExecutionPolicy Bypass -File scripts/gate_restored.ps1
```
