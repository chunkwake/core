# Fixed-tick differential: Paper vs Chunkwake

Сценарий: `FixedDiffProbe` в `ChunkwakeTest` включается через
`CHUNKWAKE_FIXED_DIFF=1`. Чанки удерживаются plugin chunk tickets, все проверки
выполняются через фиксированный счётчик тиков, без привязки к wall-clock.
Перед сценарием области redstone/hopper/fluids/fly очищаются (fly — `x=95..220`),
чтобы прошлый прогон не оставлял `moving_piston`/камни в зоне проверки.

## Финальный прогон (release, 25.11.2026, бинарь `e45ffa3`)

| сценарий | Paper 26.3 | Chunkwake |
|---|---|---|
| setup | `minY=-64 maxY=320 redstone=REDSTONE_BLOCK water=WATER fly=OBSERVER` | same |
| mob-state | `COW hp=10.0/10.0 sheep=RED` | same |
| redstone | `direct=STONE/PISTON_HEAD wire=STONE/PISTON_HEAD` | same |
| wire-data | `w11 power=14 east=side west=side`, `w12 power=15`, `piston extended=true` | same |
| hopper | `moves=3 pickups=1` | same |
| fluids | `flow=WATER fall=WATER edge=WATER meet=OBSIDIAN logged=WATER/WATER` | same |
| mob drops | `[BEEF, LEATHER]` (12 смертей, множество типов) | same |
| fly | `rear=113 blocks=8 extends=28 period=4.70 window=16` | `rear=113 blocks=10 extends=28 period=4.78 window=16` |
| fly, длинное окно t=400 | `rear=138 blocks=8 extends=78 period=4.90` | `rear=138 blocks=8 extends=77..78 period=4.92..4.95` |
| `Chunk.getEntities` | `loaded=true found-sheep=true living=1` | same |
| `Chunk.getTileEntities` | `[CHEST, HOPPER]` | same |
| stale block entity | после замены сундука камнем `[HOPPER]` | same |
| `Chunk.isSlimeChunk` | `false/false/false/false` | same |
| `Chunk.unload(false)` | `true`, `loaded=false` | same |
| `Chunk.getChunkSnapshot` | `WATER/STONE water[level=0] sky=14 emit=0 highest=200 temp=0.7389735/0.4889735 sea=-63 biome2key=minecraft:plains empty0=false empty20=true` | same |
| `ChunkSnapshot.getData` | oak stairs top/east → `4` | same |

`fly`-трейс показывает одинаковый цикл (observer → sticky pull → piston push):

```text
Paper t=24 z0=...MMSS  z1=...MMMO#
CW    t=28 z0=...OPSS  z1=...SSTO#
Paper t=28 z0=...oPMMM z1=...SS.MMM
CW    t=32 z0=...OM.SS z1=...SS.TO#
```

Обе машины едут непрерывно; после правки `finalTick`/observer задержки
позиция на t=150 совпадает (`rear=113`), окно extends t=60..140 — `16/16`.
На длинном окне t=400 позиция совпала (`rear=138`) в двух чистых прогонах;
число extends — `78/78` и `77/78`; средний период `4.92–4.95` против Paper
`4.90` (0.4–1%).
Короткое окно чувствительно к фазе старта (4.78 против 4.70), длинное — нет.
`blocks` на снимке может отличаться на 1–2: в момент отчёта одна из машин
находится в середине хода (`moving_piston`/`piston_head`), это не разная физика.

## Что исправлено в этом раунде

- `Chunk.load()` и `addPluginChunkTicket()` были no-op. Теперь ticket вызывает
  `forceload`, а `forceload add` **сразу грузит колонку** (`ensure_chunk`), даже
  без игроков. Это же исправляет `/forceload` и `Chunk.load()`.
- `World.getChunkAt(x, z)` теперь загружает чанк (Bukkit-контракт), а не просто
  отдаёт дескриптор; `Chunk.load(false)` не генерирует отсутствующий чанк.
- `Chunk.getEntities()` больше не заглушка: возвращает игроков, мобов и
  предметы своей колонки, `isEntitiesLoaded()` отражает реальную загрузку.
  Живая проверка: овца в чанке `(0,1)` находится и в Paper, и в Chunkwake
  (`found-sheep=true`).
- `Chunk.getTileEntities()` больше не заглушка: отдаёт block entity колонки
  (`blockentities` в bridge) и поддерживает predicate-фильтр. Проверка:
  `[CHEST, HOPPER]` в чанке `(1,0)` совпадает с Paper.
- `Chunk.getChunkSnapshot()` реализован end-to-end: Rust отдаёт сжатый бинарный
  снимок (block states с палитрой, биомы 4×4×4, `MOTION_BLOCKING` heightmap,
  sky/block light, температуры), Java собирает `ChunkSnapshot`. Совпадают
  блоки, `BlockData`, свет (`sky=14`, `emit=0`), высота (`200`), биомы,
  `contains`, пустые секции, `getCaptureFullTime` и температуры
  (`0.7389735` на y=0 и `0.4889735` на y=200). Попутно исправлен
  `World.getSeaLevel()`/`Login`: в flat-мире уровень моря `-63`, а не `63`.
  Для отсутствующих в снимке данных методы теперь кидают
  `IllegalStateException`, как Paper.
- `ChunkSnapshot.getData()`: сгенерирована таблица `legacy_data.tsv` на 35 723
  состояния через Paper `CraftMagicNumbers.toLegacyData`; lookup идёт по
  канонизированному `BlockData` (сортированные свойства). Проверка:
  `oak_stairs[facing=east,half=top,shape=straight,waterlogged=false]` → `4` в
  Paper и Chunkwake.
- Тикающие блоки на границе симуляции: у Paper тикет `ENTITY_TICKING`
  (уровень 31) поднимает восемь соседних чанков до `BLOCK_TICKING` (32).
  Chunkwake тикал только сам тикетный чанк, поэтому летающая машина вставала
  на `x=124`, как только её observer попадал в соседний чанк. Добавлен набор
  `block_ticking` (`simulated` + кольцо соседей) для redstone, поршней,
  observer'ов, жидкостей, hopper'ов, случайных тиков и block entity; сущности
  по-прежнему тикают только в `simulated`. Проверка: машина доезжает до
  `rear=138`, как Paper.
- Чтение блока из плоского мира во время пролива света (`light_ring`) больше
  не возвращает воздух: `block_in` ждёт `lent`-столбец и перечитывает его,
  как уже делал шумовой мир. Раньше `getBlockAt()` на один тик видел AIR
  в чанке, который в этот момент считал свет.
- Fly-машина: `PistonMovingBlockEntity.finalTick` для observer теперь
  использует чередующуюся задержку 1/2 тика вместо постоянных 2. Позиция на
  t=150 совпала (`rear=113`), окно extends t=60..140 — `16/16`; на длинном
  окне t=400 — `rear=138`, `extends=77..78`, период `4.92..4.95` против `4.90`.
- `Chunk.unload()` больше не заглушка: `unloadchunk` сохраняет (если `save=true`)
  и выгружает колонку, если она не входит в simulation/forceload. Проверка после
  загрузки дальнего чанка: Paper `unload=true loaded=false`, Chunkwake так же.
- `Chunk.isSlimeChunk()` реализован по `WorldgenRandom.seedSlimeChunk` и Spigot
  `slimeSeed=987234911`; значения на четырёх тестовых чанках совпали.
- `Chunk.load()`/`World.getChunkAt` теперь материализуют flat-чанк в `edited`
  (`load_chunk`), поэтому `isChunkLoaded` и `unload` ведут себя как у Paper.
- При смене типа блока (`chest` → `stone` и т. п.) старый block entity
  удаляется; `getTileEntities()` после замены сундука выдаёт `[HOPPER]`, как
  Paper.
- `World.save()` и `/save-all [flush]` стали синхронными: плагин (BlueMap,
  бэкапы) получает готовые файлы сразу после возврата. Раньше `World.save()`
  был no-op, а `/save-all` только поднимал флаг для фонового autosave.
- `moving_piston` теперь сохраняется как настоящий block entity
  (`minecraft:piston` + `chunkwake:state`/`chunkwake:carried`). Иначе после
  сохранения/падения в чанке навсегда оставались `moving_piston`, и летающая
  машина застревала. Восстановление проверено жёстким `halt(0)` посреди хода:
  Paper и Chunkwake поднимают машину из одного и того же состояния
  (`rear=104` на t=20) и продолжают движение.
- Запланированные block ticks (`observer`, `repeater` и т. п.) теперь
  сохраняются в чанк как ванильный `block_ticks` и восстанавливаются при
  загрузке. Без этого powered observer после рестарта оставался включённым
  навсегда и машина вставала.
- Разница `redstone wire` из прошлого отчёта оказалась состоянием тестового
  стенда: при очистке зоны перед сборкой схема совпадает бит в бит
  (`power=14`, `east=side`, `west=side`).

## Ограничения

- Остаточный период fly — `4.92..4.95` против `4.90` на длинном окне
  (0.4–1%); позиция (`rear=138`) совпадает, extends может отличаться на 1
  из-за фазы старта. На коротком окне
  абсолютная фаза зависит от порядка «плагинная задача vs redstone tick».
- `Biome.toString()` в Paper — внутренний identity-формат
  (`CraftBiome{holder=Reference{...Biome@hash}}`); differential сравнивает
  `Biome.getKey()`, который совпадает (`minecraft:plains`).
- 100% совместимость с произвольными Paper/Folia-плагинами этим сценарием не
  доказывается; это только закрывает конкретные поведенческие дыры.
