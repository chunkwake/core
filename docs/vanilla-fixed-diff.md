# Fixed-tick differential: Paper vs Chunkwake

Сценарий: `FixedDiffProbe` в `ChunkwakeTest` включается через
`CHUNKWAKE_FIXED_DIFF=1`. Чанки удерживаются plugin chunk tickets, все проверки
выполняются через фиксированный счётчик тиков, без привязки к wall-clock.

## Прогон

| сценарий | Paper 26.3 | Chunkwake |
|---|---|---|
| setup | `minY=-64 maxY=320 redstone=REDSTONE_BLOCK water=WATER fly=OBSERVER` | same |
| mob-state | `COW hp=10.0/10.0 sheep=RED` | same |
| redstone | `direct=STONE/PISTON_HEAD wire=STONE/STONE` | `direct=STONE/PISTON_HEAD wire=STONE/PISTON_HEAD` |
| hopper | `moves=3 pickups=1` | `moves=3 pickups=1` |
| fluids | `flow=WATER fall=WATER edge=WATER meet=OBSIDIAN logged=WATER/WATER` | same |
| fly | `rear=123 blocks=16 extends=29` | `rear=100 blocks=17 extends=2` |
| mob drops | `[BEEF]` | `[BEEF, LEATHER]` |

## Что исправлено

До этого `Chunk.load()`/`addPluginChunkTicket()` были no-op, из-за чего
тестовые чанки выгружались и блоки redstone/fluids/fly исчезали. Добавлен
bridge-команда `forceload <x> <z> add|remove` и реальные plugin chunk tickets.
После этого redstone (direct) и fluids совпадают с Paper, hopper тоже.

## Открытые расхождения

- **fly**: Paper перемещает observer-цепочку (`rear=123`, 29 piston extends),
  Chunkwake оставляет её на месте (`rear=100`, 2 extends). Нужна отдельная
  доработка piston/observer flying-machine физики.
- **redstone wire**: прямое питание совпадает, но wire-ветка у Paper не
  выдвигает piston, у Chunkwake — выдвигает. Требуется сверка состояния
  redstone wire/connectivity.
- **mob drops**: cow leather имеет случайный count `0..2`; в одном прогоне
  Paper выпал только `BEEF`, в Chunkwake — `BEEF+LEATHER`. Для строгого
  сравнения нужен либо фиксированный seed, либо несколько смертей и
  сравнение множества типов.

Полный `CHUNKWAKE_FIXED_DIFF`-лог: `target/fixed-paper.log` и
`target/fixed-cw.log` (рабочая директория).
