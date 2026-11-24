# Paper 26.3 vs Chunkwake: p99 MSPT и TPS (финальная release-сборка, 24.11.2026)

Строгий A/B на финальном release-бинаре Chunkwake (`4d53372`, включая
синхронный `World.save()`, загрузку чанков, восстановление `moving_piston` и
block ticks) и Paper 26.3 build 248. 5 раундов, 3 сценария, свежие
server-директории, один bot-скрипт и `PerfProbe` 1.2 на обеих сторонах.

## Условия

- AMD Ryzen 9 7945HX, 16 ядер / 32 потока, 64 GiB RAM.
- JDK 25, Paper `-Xms2G -Xmx2G`.
- Оба сервера: affinity `0xFF` (8 логических ядер), `PriorityClass=BelowNormal`.
- Сценарии: `200 cows`, `1000 cows`, `1000 cows + redstone`.
- Окно: 15 000 мс; TPS по `server_ticks` из `Server.getCurrentTick()`.
- Paper `cache/` и `versions/` заранее копируются в каждый стенд.

## Итог

| сценарий | p99 Chunkwake | p99 Paper | p99 ratio | p99 CW побед | p(CW лучше) | TPS CW / Paper |
|---|---:|---:|---:|---:|---:|---:|
| 200 cows | **0.759** | 3.403 | **0.223x** | **5/5** | **0.031** | 19.99 / 20.02 |
| 1000 cows | **0.685** | 12.162 | **0.056x** | **5/5** | **0.031** | 19.99 / 20.00 |
| 1000 cows + redstone | **0.781** | 11.343 | **0.069x** | **5/5** | **0.031** | 19.99 / 20.00 |

p99 — pooled по 5 окнам (по 301–325 тиков каждое). Во всех 15 парных окнах
p99 Chunkwake ниже Paper; парный односторонний знаковый тест даёт p=0.031.

## Сырые данные

- 30 CSV: `benchmarks/ab-p99/samples/{chunkwake,paper}/samples-rN-<scenario>.csv`.
- Полная таблица: `benchmarks/ab-p99/results.md`.
- Логи прогона: `target/ab-p99/r1..r5`, вывод: `target/ab-p99/run_final2.txt`.

## Ограничения

- Нагрузка из мобов/redstone, без игроков, чанков, AI-путей и сети.
- TPS упирается в 20.00 — деградации нет, но преимущество по TPS не измеряется.
- 100% совместимость с произвольными Paper/Folia-плагинами не доказана; см.
  `docs/vanilla-diff.md`, `docs/vanilla-fixed-diff.md` и plugin gate.

---

# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 301 | 301 | 0.435 | 0.677 | 0.826 | 0.902 | 20.00 | 20.00 |
| 1 | Chunkwake | cows1000 | 301 | 301 | 0.383 | 0.548 | 0.636 | 0.895 | 20.01 | 20.00 |
| 1 | Chunkwake | redstone | 325 | 325 | 0.406 | 0.703 | 0.932 | 1.194 | 20.02 | 20.00 |
| 1 | Paper | cows200 | 301 | 301 | 2.310 | 3.046 | 3.394 | 7.832 | 20.00 | 19.74 |
| 1 | Paper | cows1000 | 301 | 301 | 8.413 | 9.963 | 12.127 | 35.359 | 20.00 | 20.00 |
| 1 | Paper | redstone | 325 | 325 | 8.139 | 10.298 | 12.418 | 44.486 | 20.00 | 20.00 |
| 2 | Chunkwake | cows200 | 301 | 301 | 0.376 | 0.617 | 0.759 | 1.442 | 20.01 | 20.00 |
| 2 | Chunkwake | cows1000 | 301 | 301 | 0.381 | 0.598 | 0.691 | 0.713 | 20.00 | 20.00 |
| 2 | Chunkwake | redstone | 326 | 326 | 0.396 | 0.595 | 0.646 | 0.917 | 20.00 | 20.00 |
| 2 | Paper | cows200 | 302 | 302 | 2.271 | 3.007 | 3.403 | 5.015 | 20.05 | 19.59 |
| 2 | Paper | cows1000 | 301 | 301 | 7.675 | 9.585 | 12.162 | 31.953 | 19.99 | 20.00 |
| 2 | Paper | redstone | 324 | 324 | 6.891 | 8.643 | 10.210 | 36.714 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 301 | 301 | 0.420 | 0.624 | 0.687 | 0.787 | 19.95 | 20.00 |
| 3 | Chunkwake | cows1000 | 301 | 301 | 0.435 | 0.655 | 0.720 | 0.791 | 20.00 | 20.00 |
| 3 | Chunkwake | redstone | 325 | 325 | 0.474 | 0.700 | 0.870 | 1.237 | 19.96 | 20.00 |
| 3 | Paper | cows200 | 301 | 301 | 2.243 | 2.957 | 3.322 | 4.043 | 20.00 | 19.69 |
| 3 | Paper | cows1000 | 301 | 301 | 8.170 | 9.815 | 12.024 | 33.917 | 20.01 | 20.00 |
| 3 | Paper | redstone | 325 | 325 | 6.823 | 8.692 | 10.291 | 37.696 | 20.00 | 20.00 |
| 4 | Chunkwake | cows200 | 301 | 301 | 0.344 | 0.549 | 0.764 | 0.822 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 300 | 300 | 0.366 | 0.486 | 0.556 | 0.644 | 19.94 | 20.00 |
| 4 | Chunkwake | redstone | 320 | 320 | 0.384 | 0.474 | 0.529 | 0.598 | 19.97 | 20.00 |
| 4 | Paper | cows200 | 301 | 301 | 2.109 | 2.959 | 3.427 | 4.644 | 19.99 | 19.85 |
| 4 | Paper | cows1000 | 301 | 301 | 6.903 | 9.573 | 11.359 | 12.484 | 19.99 | 20.00 |
| 4 | Paper | redstone | 325 | 325 | 6.919 | 9.768 | 10.963 | 38.695 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 301 | 301 | 0.277 | 0.430 | 0.455 | 0.474 | 20.00 | 20.00 |
| 5 | Chunkwake | cows1000 | 301 | 301 | 0.289 | 0.462 | 0.570 | 0.697 | 20.00 | 20.00 |
| 5 | Chunkwake | redstone | 323 | 323 | 0.303 | 0.516 | 0.686 | 0.749 | 20.01 | 20.00 |
| 5 | Paper | cows200 | 302 | 302 | 2.185 | 2.813 | 3.211 | 4.795 | 20.05 | 19.81 |
| 5 | Paper | cows1000 | 301 | 301 | 7.442 | 10.572 | 12.764 | 29.148 | 20.00 | 20.00 |
| 5 | Paper | redstone | 324 | 324 | 6.755 | 9.044 | 9.881 | 36.182 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1505 | 0.396 | 0.380 | 0.610 | 0.759 | 0.759 | 1.442 | 19.99 | 19.95 | 20.00 |
| cows200 | Paper | 5 | 1507 | 2.291 | 2.222 | 2.969 | 3.403 | 3.394 | 7.832 | 20.02 | 19.99 | 19.74 |
| cows1000 | Chunkwake | 5 | 1504 | 0.388 | 0.378 | 0.564 | 0.685 | 0.636 | 0.895 | 19.99 | 19.94 | 20.00 |
| cows1000 | Paper | 5 | 1505 | 8.051 | 7.956 | 9.923 | 12.162 | 12.127 | 35.359 | 20.00 | 19.99 | 20.00 |
| redstone | Chunkwake | 5 | 1619 | 0.411 | 0.396 | 0.618 | 0.781 | 0.686 | 1.237 | 19.99 | 19.96 | 20.00 |
| redstone | Paper | 5 | 1623 | 7.444 | 7.025 | 9.581 | 11.343 | 10.291 | 44.486 | 20.00 | 20.00 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -2.653 / -2.644 | 5/5 | 0.031 | 1.000 | 0.759 / 3.403 | 0.223x | -0.03 | 1/5 | 0.938 | 0.312 |
| cows1000 | 5 | -11.452 / -11.470 | 5/5 | 0.031 | 1.000 | 0.685 / 12.162 | 0.056x | -0.01 | 2/5 | 0.688 | 0.688 |
| redstone | 5 | -10.020 / -9.564 | 5/5 | 0.031 | 1.000 | 0.781 / 11.343 | 0.069x | -0.01 | 2/5 | 0.688 | 0.688 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
