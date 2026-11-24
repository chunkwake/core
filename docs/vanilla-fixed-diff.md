# Fixed-tick differential: Paper vs Chunkwake

Сценарий: `FixedDiffProbe` в `ChunkwakeTest` включается через
`CHUNKWAKE_FIXED_DIFF=1`. Чанки удерживаются plugin chunk tickets, все проверки
выполняются через фиксированный счётчик тиков, без привязки к wall-clock.

## Текущий прогон

| сценарий | Paper 26.3 | Chunkwake |
|---|---|---|
| setup | `minY=-64 maxY=320 redstone=REDSTONE_BLOCK water=WATER fly=OBSERVER` | same |
| mob-state | `COW hp=10.0/10.0 sheep=RED` | same |
| redstone | `direct=STONE/PISTON_HEAD wire=STONE/STONE` | `direct=STONE/PISTON_HEAD wire=STONE/PISTON_HEAD` |
| hopper | `moves=3 pickups=1` | same |
| fluids | `flow=WATER fall=WATER edge=WATER meet=OBSIDIAN logged=WATER/WATER` | same |
| fly | `rear=123 blocks=16 extends=29` | `rear=100 blocks=17 extends=2` |
| mob drops | `[BEEF, LEATHER]` | same |

## Что исправлено

- `Chunk.load()`/`addPluginChunkTicket()` были no-op; добавлены реальные
  plugin chunk tickets и bridge-команда `forceload <x> <z> add|remove`.
- После этого совпали redstone direct, hopper и fluids.
- Cow drop теперь проверяется по **множеству типов** на 12 смертях, поэтому
  случайный `leather 0..2` больше не даёт ложный diff; обе стороны дают
  `[BEEF, LEATHER]`.

## Открытые расхождения

- **fly**: Paper перемещает observer/piston-цепочку (`rear=123`, 29 extends),
  Chunkwake оставляет её на месте (`rear=100`, 2 extends). Нужна
  piston/observer flying-machine физика.
- **redstone wire**: прямое питание совпадает; wire-ветка у Chunkwake
  выдвигает piston, у Paper — нет. Нужна сверка connectivity/update порядка.
- Полный gate и A/B не заменяют эти сценарии; 100% vanilla parity не
  доказан.
