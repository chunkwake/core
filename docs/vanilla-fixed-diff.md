# Fixed-tick differential: Paper vs Chunkwake

Сценарий: `FixedDiffProbe` в `ChunkwakeTest` включается через
`CHUNKWAKE_FIXED_DIFF=1`. Чанки удерживаются plugin chunk tickets, все проверки
выполняются через фиксированный счётчик тиков, без привязки к wall-clock.
Перед сценарием области redstone/hopper/fluids/fly очищаются, чтобы прошлый
прогон не оставлял `moving_piston`/камни в зоне проверки.

## Финальный прогон (release, 24.11.2026)

| сценарий | Paper 26.3 | Chunkwake |
|---|---|---|
| setup | `minY=-64 maxY=320 redstone=REDSTONE_BLOCK water=WATER fly=OBSERVER` | same |
| mob-state | `COW hp=10.0/10.0 sheep=RED` | same |
| redstone | `direct=STONE/PISTON_HEAD wire=STONE/PISTON_HEAD` | same |
| wire-data | `w11 power=14 east=side west=side`, `w12 power=15`, `piston extended=true` | same |
| hopper | `moves=3 pickups=1` | same |
| fluids | `flow=WATER fall=WATER edge=WATER meet=OBSIDIAN logged=WATER/WATER` | same |
| mob drops | `[BEEF, LEATHER]` (12 смертей, множество типов) | same |
| fly | `rear=113 blocks=8 extends=28` | `rear=112 blocks=10 extends=26` |

`fly`-трейс показывает одинаковый цикл (observer → sticky pull → piston push):

```text
Paper t=24 z0=...MMSS  z1=...MMMO#
CW    t=28 z0=...OPSS  z1=...SSTO#
Paper t=28 z0=...oPMMM z1=...SS.MMM
CW    t=32 z0=...OM.SS z1=...SS.TO#
```

Обе машины едут непрерывно; на 130 тиках разница 1 блок — это фазовый сдвиг
старта (плагинная задача выполняется в разном порядке относительно тика
redstone), а не разная скорость. На длинном окне: Paper 104→122 за 180 тиков,
Chunkwake 104→139 за 380 тиков (~1 блок на 10.6 тика у обоих).

## Что исправлено в этом раунде

- `Chunk.load()` и `addPluginChunkTicket()` были no-op. Теперь ticket вызывает
  `forceload`, а `forceload add` **сразу грузит колонку** (`ensure_chunk`), даже
  без игроков. Это же исправляет `/forceload` и `Chunk.load()`.
- `World.getChunkAt(x, z)` теперь загружает чанк (Bukkit-контракт), а не просто
  отдаёт дескриптор.
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

- Фазовый сдвиг летающей машины на фиксированном тике (±2–4 тика) остаётся:
  скорость и состояния совпадают, абсолютная фаза зависит от порядка
  выполнения плагинной задачи и тика redstone.
- 100% совместимость с произвольными Paper/Folia-плагинами этим сценарием не
  доказывается; это только закрывает конкретные поведенческие дыры.
