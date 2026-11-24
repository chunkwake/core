# Paper 26.3 vs Chunkwake: p99 MSPT и TPS (финальная release-сборка, 24.11.2026)

Строгий A/B на финальном release-бинаре Chunkwake (коммит `5f386ee`) и
Paper 26.3 build 248. 5 раундов, 3 сценария, свежие server-директории на
каждый запуск, один и тот же bot-скрипт и `PerfProbe` 1.2 на обеих сторонах.

## Условия

- AMD Ryzen 9 7945HX, 16 ядер / 32 потока, 64 GiB RAM.
- JDK 25, Paper `-Xms2G -Xmx2G`.
- Оба сервера запускались с `ProcessorAffinity=0xFF` (8 логических ядер) и
  `PriorityClass=BelowNormal`, чтобы не занимать рабочую машину на 100%.
  Это paired-прогон, а не максимальный throughput-режим всей машины.
- Сценарии: `200 cows`, `1000 cows`, `1000 cows + redstone`.
- Окно измерения: 15 000 мс; TPS считается по `server_ticks` из
  `Server.getCurrentTick()`.
- Для Paper предварительно копируются `cache/` и `versions/`, чтобы сбой
  Paperclip-загрузки не ломал раунд.

## Итог

| сценарий | p99 Chunkwake | p99 Paper | p99 ratio | p99 CW побед | p(CW лучше) | TPS CW / Paper |
|---|---:|---:|---:|---:|---:|---:|
| 200 cows | **0.700** | 3.370 | **0.208x** | **5/5** | **0.031** | 19.99 / 20.02 |
| 1000 cows | **0.725** | 10.578 | **0.069x** | **5/5** | **0.031** | 20.01 / 20.02 |
| 1000 cows + redstone | **0.684** | 9.761 | **0.070x** | **5/5** | **0.031** | 20.00 / 20.01 |

p99 — pooled по 5 окнам. Во всех 15 парных окнах p99 Chunkwake ниже Paper;
TPS в обоих лагерях около 20.00.

## Сырые данные

- 30 CSV-файлов: `benchmarks/ab-p99/samples/{chunkwake,paper}/samples-rN-<scenario>.csv`.
- Полная таблица 30 окон: `benchmarks/ab-p99/results.md`.
- Логи и PerfProbe-файлы прогона: `target/ab-p99/r1..r5`.

## Ограничения

- Нагрузка: мобы и redstone, без игроков, чанков, AI-путей и реального
  сетевого трафика.
- TPS упирается в 20.00; сравнение по TPS здесь показывает отсутствие
  деградации, а не преимущество.
- 100% совместимость с произвольными Paper/Folia-плагинами этим прогоном не
  доказывается; для неё существует отдельный plugin gate 100/100 и
  differential-отчёт `docs/vanilla-diff.md`.

---

# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 301 | 301 | 0.414 | 0.551 | 0.673 | 0.772 | 20.00 | 20.00 |
| 1 | Chunkwake | cows1000 | 301 | 301 | 0.281 | 0.444 | 0.554 | 0.631 | 20.00 | 20.00 |
| 1 | Chunkwake | redstone | 324 | 323 | 0.338 | 0.508 | 0.640 | 1.368 | 19.95 | 20.00 |
| 1 | Paper | cows200 | 301 | 301 | 2.121 | 2.740 | 3.149 | 3.535 | 20.02 | 19.84 |
| 1 | Paper | cows1000 | 301 | 302 | 8.094 | 9.978 | 11.591 | 18.205 | 20.06 | 20.00 |
| 1 | Paper | redstone | 323 | 323 | 7.697 | 9.286 | 10.783 | 26.603 | 19.99 | 20.00 |
| 2 | Chunkwake | cows200 | 301 | 301 | 0.408 | 0.543 | 0.647 | 0.696 | 20.00 | 20.00 |
| 2 | Chunkwake | cows1000 | 301 | 301 | 0.326 | 0.494 | 0.550 | 0.651 | 20.00 | 20.00 |
| 2 | Chunkwake | redstone | 325 | 325 | 0.389 | 0.522 | 0.594 | 0.651 | 20.03 | 20.00 |
| 2 | Paper | cows200 | 301 | 301 | 2.214 | 2.828 | 3.358 | 4.465 | 20.00 | 19.89 |
| 2 | Paper | cows1000 | 301 | 301 | 7.439 | 9.190 | 9.919 | 19.959 | 20.00 | 20.00 |
| 2 | Paper | redstone | 325 | 325 | 6.481 | 7.266 | 7.684 | 10.265 | 20.01 | 20.00 |
| 3 | Chunkwake | cows200 | 301 | 301 | 0.466 | 0.655 | 0.718 | 1.047 | 20.00 | 20.00 |
| 3 | Chunkwake | cows1000 | 302 | 302 | 0.477 | 0.641 | 0.751 | 0.801 | 20.05 | 20.00 |
| 3 | Chunkwake | redstone | 321 | 321 | 0.462 | 0.636 | 0.773 | 1.636 | 20.00 | 20.00 |
| 3 | Paper | cows200 | 301 | 301 | 2.186 | 2.961 | 3.955 | 14.813 | 20.00 | 19.90 |
| 3 | Paper | cows1000 | 301 | 301 | 7.159 | 9.341 | 10.012 | 20.301 | 20.01 | 20.00 |
| 3 | Paper | redstone | 325 | 325 | 6.854 | 8.582 | 9.629 | 21.390 | 20.05 | 20.00 |
| 4 | Chunkwake | cows200 | 301 | 301 | 0.366 | 0.587 | 0.749 | 1.086 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 301 | 301 | 0.487 | 0.640 | 0.795 | 0.830 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 323 | 323 | 0.306 | 0.491 | 0.547 | 0.633 | 20.03 | 20.00 |
| 4 | Paper | cows200 | 300 | 301 | 2.358 | 3.015 | 3.275 | 3.794 | 20.02 | 19.90 |
| 4 | Paper | cows1000 | 301 | 301 | 6.700 | 7.326 | 8.644 | 19.616 | 20.01 | 20.00 |
| 4 | Paper | redstone | 325 | 325 | 6.282 | 6.773 | 7.365 | 22.328 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 300 | 300 | 0.486 | 0.622 | 0.688 | 0.789 | 19.94 | 20.00 |
| 5 | Chunkwake | cows1000 | 301 | 301 | 0.405 | 0.558 | 0.592 | 0.652 | 20.00 | 20.00 |
| 5 | Chunkwake | redstone | 324 | 324 | 0.421 | 0.603 | 0.783 | 1.874 | 20.01 | 20.00 |
| 5 | Paper | cows200 | 302 | 302 | 2.590 | 3.100 | 3.370 | 4.074 | 20.05 | 19.87 |
| 5 | Paper | cows1000 | 301 | 301 | 7.032 | 7.838 | 9.146 | 18.689 | 20.00 | 20.00 |
| 5 | Paper | redstone | 325 | 325 | 7.187 | 8.606 | 9.810 | 26.421 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1504 | 0.433 | 0.431 | 0.614 | 0.700 | 0.688 | 1.086 | 19.99 | 19.94 | 20.00 |
| cows200 | Paper | 5 | 1505 | 2.349 | 2.295 | 2.974 | 3.370 | 3.358 | 14.813 | 20.02 | 20.00 | 19.88 |
| cows1000 | Chunkwake | 5 | 1506 | 0.406 | 0.400 | 0.591 | 0.725 | 0.592 | 0.830 | 20.01 | 20.00 | 20.00 |
| cows1000 | Paper | 5 | 1505 | 7.460 | 7.235 | 9.428 | 10.578 | 9.919 | 20.301 | 20.02 | 20.00 | 20.00 |
| redstone | Chunkwake | 5 | 1617 | 0.395 | 0.386 | 0.576 | 0.684 | 0.640 | 1.874 | 20.00 | 19.95 | 20.00 |
| redstone | Paper | 5 | 1623 | 7.013 | 6.787 | 8.432 | 9.761 | 9.629 | 26.603 | 20.01 | 19.99 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -2.726 / -2.682 | 5/5 | 0.031 | 1.000 | 0.700 / 3.370 | 0.208x | -0.03 | 0/5 | 1.000 | 0.125 |
| cows1000 | 5 | -9.214 / -9.261 | 5/5 | 0.031 | 1.000 | 0.725 / 10.578 | 0.069x | -0.01 | 1/5 | 0.875 | 0.500 |
| redstone | 5 | -8.387 / -8.855 | 5/5 | 0.031 | 1.000 | 0.684 / 9.761 | 0.070x | -0.01 | 3/5 | 0.500 | 0.812 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
