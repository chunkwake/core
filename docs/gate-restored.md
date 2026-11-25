# Восстановленный plugin gate (25.11.2026)

Исходный стенд `load-gate-extra` был удалён вместе с диском C:. Стенд собран
заново из Modrinth-плагинов для Paper/Bukkit/Spigot 26.3 и локальных
test/probe-плагинов.

## Результат

```
plugins: enabled 100 plugin(s) in 44723 ms
FAILED: (пусто)
LOADERR: (пусто)
SKIPPED: (пусто)
PANICS: 0
JARS: 104
PROBES: folia=True cw-folia=True
PROBE-FAIL: (пусто)
```

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
- Базовый `load-gate-server` (27/27) этим прогоном не восстанавливался.

## Воспроизведение

```powershell
powershell -ExecutionPolicy Bypass -File scripts/restore_gate_plugins.ps1
powershell -ExecutionPolicy Bypass -File scripts/gate_restored.ps1
```
