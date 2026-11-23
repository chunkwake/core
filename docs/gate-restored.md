# Восстановленный plugin gate (23.11.2026)

Исходный стенд `load-gate-extra` был удалён вместе с диском C:. Стенд собран
заново из Modrinth-плагинов для Paper/Bukkit/Spigot 26.3 и локальных
test/probe-плагинов.

## Результат

```
83 of 83 enabled, 1 skipped (missing server internals) in 36754 ms
FAILED: (пусто)
LOADERR: (пусто)
PANICS: 0
JARS: 88
```

`JARS: 88` — 83 плагина + 5 библиотечных/зависимых jar-ов, которые Chunkwake
не считает отдельными плагинами.

Полный список файлов:
[gate-restored-plugins.txt](gate-restored-plugins.txt).

## Важно

- Это **не побайтно тот же** набор, что старый 83/83: старые jar-ы утеряны.
  Набор собран заново из доступных 26.3-версий; часть проектов заменилась
  более новыми сборками.
- `journeymap` (клиентский мод), `QuickShop-Hikari` (конфликт WorldEdit),
  `EconomyShopGUI`, `ClickVillagers`, `FreedomChat`, `NexusCore`,
  `NexusAuctionHouse`, `DonutAuctionHouse`, `BKCommonLib` были исключены:
  они падали на этом стенде.
- CE по-прежнему включается, но data pack падает на NMS-прокси
  (`ShapedRecipePattern`, `InjectedWorldBorder`, `BlockState.blockState`).
  Этот gate проверяет загрузку/enable плагинов, а не CE parity.
- Базовый `load-gate-server` (27/27) этим прогоном не восстанавливался.

## Воспроизведение

```powershell
powershell -ExecutionPolicy Bypass -File scripts/restore_gate_plugins.ps1
powershell -ExecutionPolicy Bypass -File scripts/gate_restored.ps1
```
