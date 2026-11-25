# Paper 26.3 vs Chunkwake: p99 MSPT и TPS (plugin-driven A/B, 25.11.2026)

Строгий A/B на release-бинаре Chunkwake `270674c` (ChunkSnapshot.getData,
legacy-data на 35 723 состояния, unload/isSlimeChunk/stale BE) и Paper 26.3
build 248. Нагрузку генерирует `AbLoadProbe` внутри тестового плагина:
200/1000 коров и redstone-линия; `opcmd_probe` только держит игрока в мире,
чтобы spawn-зоны оставались simulated. 5 раундов, 3 сценария, свежие
server-директории.

## Условия

- AMD Ryzen 9 7945HX, 16 ядер / 32 потока, 64 GiB RAM.
- JDK 25, Paper `-Xms2G -Xmx2G`.
- Оба сервера: affinity `0xFF`, `PriorityClass=BelowNormal`.
- Сценарии: `200 cows`, `1000 cows`, `1000 cows + redstone`.
- Окно: 15 000 мс (300 тиков); TPS по `server_ticks`.
- Нагрузка плагинная: коровы спавнятся батчами по 20/тик, redstone строится
  и переключается задачами плагина.

## Итог

| сценарий | p99 Chunkwake | p99 Paper | p99 ratio | p99 CW побед | p(CW лучше) | TPS CW / Paper |
|---|---:|---:|---:|---:|---:|---:|
| 200 cows | **1.188** | 50.724 | **0.023x** | **5/5** | **0.031** | 20.00 / 20.00 |
| 1000 cows | **1.889** | 13.879 | **0.136x** | **5/5** | **0.031** | 20.00 / 20.00 |
| 1000 cows + redstone | **3.145** | 14.137 | **0.222x** | **5/5** | **0.031** | 20.00 / 20.00 |

p99 — pooled по 5 окнам (300 тиков каждое). Во всех 15 парных окнах p99
Chunkwake ниже Paper; парный односторонний знаковый тест p=0.031.

## Отличие от bot-driven прогона

Предыдущий A/B (`aee780b`/`1470a76`) использовал `opcmd_probe` для всех команд.
Текущий прогон — plugin-driven: это надёжнее в общем стенде, но абсолютные
числа и p99 не сравниваются напрямую с bot-driven серией. Внутри текущей серии
Paper и Chunkwake используют один и тот же плагин и одинаковые окна.

## Сырые данные

- 30 CSV: `benchmarks/ab-p99/samples/{chunkwake,paper}/samples-rN-<scenario>.csv`.
- Полная таблица: `benchmarks/ab-p99/results.md`.
- Логи: `target/ab-p99-plugin/r1..r5`, вывод: `target/ab-p99-plugin/run.txt`.

## Ограничения

- Нагрузка синтетическая: без реальных игроков, AI-путей, сети и чанков
  игроков (idle-бот только удерживает spawn-зоны).
- A/B снят на `270674c` до fly-правки `finalTick`/observer; в нагрузке нет
  observer-машин, поэтому эта правка инертна для данных окон.
- TPS упирается в 20.00 — преимущество по TPS не измеряется.
- 100% совместимость с произвольными Paper/Folia-плагинами не доказана;
  см. `docs/vanilla-diff.md`, `docs/vanilla-fixed-diff.md` и plugin gate.

---

# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99-plugin`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 300 | 300 | 0.628 | 1.056 | 2.208 | 2.696 | 20.00 | 20.00 |
| 1 | Chunkwake | cows1000 | 300 | 300 | 1.199 | 1.640 | 1.989 | 2.667 | 20.00 | 20.00 |
| 1 | Chunkwake | redstone | 300 | 300 | 1.225 | 1.729 | 2.031 | 2.252 | 20.00 | 20.00 |
| 1 | Paper | cows200 | 300 | 300 | 3.498 | 6.618 | 60.823 | 133.653 | 20.00 | 19.83 |
| 1 | Paper | cows1000 | 300 | 300 | 9.393 | 13.356 | 14.728 | 126.981 | 20.00 | 19.90 |
| 1 | Paper | redstone | 300 | 300 | 8.761 | 11.055 | 13.039 | 39.872 | 20.00 | 19.90 |
| 2 | Chunkwake | cows200 | 300 | 300 | 0.683 | 0.970 | 1.066 | 2.515 | 20.00 | 20.00 |
| 2 | Chunkwake | cows1000 | 300 | 300 | 1.324 | 1.673 | 1.880 | 2.130 | 20.00 | 20.00 |
| 2 | Chunkwake | redstone | 300 | 300 | 1.407 | 3.145 | 11.026 | 16.733 | 20.00 | 20.00 |
| 2 | Paper | cows200 | 300 | 300 | 3.476 | 6.667 | 47.949 | 174.904 | 20.00 | 19.79 |
| 2 | Paper | cows1000 | 300 | 300 | 8.835 | 11.380 | 16.420 | 38.546 | 20.00 | 20.00 |
| 2 | Paper | redstone | 300 | 300 | 8.476 | 10.901 | 15.082 | 47.841 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 300 | 300 | 0.609 | 0.823 | 0.896 | 2.658 | 20.00 | 20.00 |
| 3 | Chunkwake | cows1000 | 300 | 300 | 1.205 | 1.574 | 1.883 | 4.455 | 20.00 | 20.00 |
| 3 | Chunkwake | redstone | 300 | 300 | 1.195 | 1.548 | 1.951 | 2.413 | 20.00 | 20.00 |
| 3 | Paper | cows200 | 300 | 300 | 3.315 | 6.006 | 28.201 | 173.145 | 20.00 | 19.69 |
| 3 | Paper | cows1000 | 300 | 300 | 8.806 | 10.689 | 11.774 | 37.424 | 20.00 | 20.00 |
| 3 | Paper | redstone | 300 | 300 | 8.238 | 9.958 | 11.321 | 38.249 | 20.00 | 20.00 |
| 4 | Chunkwake | cows200 | 300 | 300 | 0.569 | 0.852 | 0.959 | 3.113 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 300 | 300 | 1.123 | 1.490 | 1.687 | 2.425 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 300 | 300 | 1.074 | 1.466 | 1.616 | 1.963 | 20.00 | 20.00 |
| 4 | Paper | cows200 | 300 | 300 | 3.773 | 7.004 | 71.957 | 120.508 | 20.00 | 19.70 |
| 4 | Paper | cows1000 | 300 | 300 | 10.515 | 12.453 | 13.338 | 43.295 | 20.00 | 20.00 |
| 4 | Paper | redstone | 300 | 300 | 8.974 | 11.666 | 14.532 | 39.942 | 20.01 | 20.00 |
| 5 | Chunkwake | cows200 | 300 | 300 | 0.534 | 0.755 | 0.867 | 2.654 | 20.00 | 20.00 |
| 5 | Chunkwake | cows1000 | 300 | 300 | 1.246 | 1.621 | 1.857 | 2.180 | 20.00 | 20.00 |
| 5 | Chunkwake | redstone | 300 | 300 | 1.194 | 1.543 | 1.699 | 2.221 | 19.99 | 20.00 |
| 5 | Paper | cows200 | 300 | 300 | 3.616 | 5.749 | 33.039 | 235.977 | 20.01 | 19.77 |
| 5 | Paper | cows1000 | 300 | 300 | 9.255 | 11.186 | 12.854 | 13.760 | 20.00 | 20.00 |
| 5 | Paper | redstone | 300 | 300 | 8.730 | 11.698 | 14.561 | 42.733 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1500 | 0.640 | 0.602 | 0.909 | 1.188 | 0.959 | 3.113 | 20.00 | 20.00 | 20.00 |
| cows200 | Paper | 5 | 1500 | 5.359 | 3.563 | 6.480 | 50.724 | 47.949 | 235.977 | 20.00 | 20.00 | 19.76 |
| cows1000 | Chunkwake | 5 | 1500 | 1.253 | 1.222 | 1.603 | 1.889 | 1.880 | 4.455 | 20.00 | 20.00 | 20.00 |
| cows1000 | Paper | 5 | 1500 | 9.809 | 9.301 | 12.132 | 13.879 | 13.338 | 126.981 | 20.00 | 20.00 | 19.98 |
| redstone | Chunkwake | 5 | 1500 | 1.327 | 1.215 | 1.764 | 3.145 | 1.951 | 16.733 | 20.00 | 19.99 | 20.00 |
| redstone | Paper | 5 | 1500 | 9.064 | 8.636 | 11.301 | 14.137 | 14.532 | 47.841 | 20.00 | 20.00 | 19.98 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -47.195 / -46.883 | 5/5 | 0.031 | 1.000 | 1.188 / 50.724 | 0.023x | -0.00 | 0/5 | 1.000 | 0.500 |
| cows1000 | 5 | -11.964 / -11.651 | 5/5 | 0.031 | 1.000 | 1.889 / 13.879 | 0.136x | 0.00 | 0/5 | 1.000 | 1.000 |
| redstone | 5 | -10.042 / -11.008 | 5/5 | 0.031 | 1.000 | 3.145 / 14.137 | 0.222x | -0.00 | 0/5 | 1.000 | 0.250 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
