# Восстановленный plugin gate (23.11.2026)

Исходный стенд `load-gate-extra` был удалён вместе с диском C:. Стенд собран
заново из Modrinth-плагинов для Paper/Bukkit/Spigot 26.3 и локальных
test/probe-плагинов.

## Результат

```
91 of 91 enabled in 82840 ms
FAILED: (пусто)
LOADERR: (пусто)
SKIPPED: (пусто)
PANICS: 0
JARS: 95
```

`JARS: 95` — 91 плагин + библиотечные/зависимые jar-ы, которые Chunkwake
не считает отдельными плагинами. CommandAPI и Pl3xMap больше не пропускаются:
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
  загрузку/enable плагинов и отсутствие runtime-ошибок; полная parity
  произвольных плагинов им не доказывается.
- Базовый `load-gate-server` (27/27) этим прогоном не восстанавливался.

## Воспроизведение

```powershell
powershell -ExecutionPolicy Bypass -File scripts/restore_gate_plugins.ps1
powershell -ExecutionPolicy Bypass -File scripts/gate_restored.ps1
```
