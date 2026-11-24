# Paper 26.3 vs Chunkwake: p99 MSPT и TPS (финальная release-сборка, 24.11.2026)

Строгий A/B на финальном release-бинаре Chunkwake (`1470a76`: синхронный
`World.save()`, загрузка чанков, `Chunk.getEntities()`, восстановление
`moving_piston` и block ticks) и Paper 26.3 build 248. 5 раундов, 3 сценария,
свежие server-директории, один bot-скрипт и `PerfProbe` 1.2 на обеих сторонах.

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
| 200 cows | **0.768** | 3.957 | **0.194x** | **5/5** | **0.031** | 20.00 / 20.02 |
| 1000 cows | **0.786** | 12.949 | **0.061x** | **5/5** | **0.031** | 20.00 / 20.02 |
| 1000 cows + redstone | **0.800** | 12.216 | **0.066x** | **5/5** | **0.031** | 19.99 / 20.01 |

p99 — pooled по 5 окнам (по 301–325 тиков каждое). Во всех 15 парных окнах
p99 Chunkwake ниже Paper; парный односторонний знаковый тест даёт p=0.031.

## Сырые данные

- 30 CSV: `benchmarks/ab-p99/samples/{chunkwake,paper}/samples-rN-<scenario>.csv`.
- Полная таблица: `benchmarks/ab-p99/results.md`.
- Логи прогона: `target/ab-p99/r1..r5`, вывод: `target/ab-p99/run_final3.txt`.

## Ограничения

- A/B снят на `1470a76`; последующая правка `Chunk.getTileEntities` — только
  API-поверхность и не затрагивает тиковый путь ядра.
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
| 1 | Chunkwake | cows200 | 300 | 300 | 0.363 | 0.570 | 0.669 | 0.776 | 19.94 | 20.00 |
| 1 | Chunkwake | cows1000 | 301 | 301 | 0.364 | 0.539 | 0.653 | 0.818 | 20.02 | 20.00 |
| 1 | Chunkwake | redstone | 328 | 328 | 0.364 | 0.561 | 0.714 | 0.741 | 19.99 | 20.00 |
| 1 | Paper | cows200 | 301 | 301 | 2.412 | 3.080 | 3.457 | 4.036 | 20.00 | 19.78 |
| 1 | Paper | cows1000 | 301 | 301 | 8.794 | 10.733 | 12.151 | 32.604 | 20.02 | 20.00 |
| 1 | Paper | redstone | 325 | 325 | 7.945 | 9.330 | 10.097 | 45.526 | 20.02 | 20.00 |
| 2 | Chunkwake | cows200 | 300 | 301 | 0.333 | 0.517 | 0.605 | 0.862 | 20.02 | 20.00 |
| 2 | Chunkwake | cows1000 | 301 | 301 | 0.320 | 0.554 | 0.761 | 0.922 | 20.00 | 20.00 |
| 2 | Chunkwake | redstone | 324 | 326 | 0.326 | 0.494 | 0.577 | 0.778 | 20.02 | 20.00 |
| 2 | Paper | cows200 | 301 | 301 | 2.296 | 3.037 | 3.617 | 6.359 | 20.00 | 19.77 |
| 2 | Paper | cows1000 | 301 | 301 | 6.916 | 8.678 | 10.318 | 31.623 | 20.01 | 20.00 |
| 2 | Paper | redstone | 325 | 325 | 7.132 | 8.498 | 9.562 | 37.219 | 20.03 | 20.00 |
| 3 | Chunkwake | cows200 | 302 | 302 | 0.477 | 0.695 | 0.764 | 0.883 | 20.05 | 20.00 |
| 3 | Chunkwake | cows1000 | 301 | 301 | 0.454 | 0.673 | 0.871 | 0.982 | 19.99 | 20.00 |
| 3 | Chunkwake | redstone | 324 | 324 | 0.451 | 0.681 | 1.049 | 1.627 | 19.97 | 20.00 |
| 3 | Paper | cows200 | 302 | 302 | 2.414 | 3.177 | 3.426 | 3.908 | 20.06 | 19.66 |
| 3 | Paper | cows1000 | 302 | 302 | 8.593 | 10.241 | 11.830 | 32.345 | 20.05 | 20.00 |
| 3 | Paper | redstone | 325 | 325 | 8.167 | 9.712 | 11.278 | 37.453 | 20.01 | 20.00 |
| 4 | Chunkwake | cows200 | 301 | 301 | 0.481 | 0.691 | 0.817 | 1.270 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 301 | 301 | 0.400 | 0.577 | 0.793 | 1.146 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 323 | 324 | 0.433 | 0.638 | 0.859 | 4.130 | 19.97 | 20.00 |
| 4 | Paper | cows200 | 301 | 301 | 2.620 | 3.514 | 5.101 | 6.098 | 20.00 | 19.73 |
| 4 | Paper | cows1000 | 301 | 301 | 7.844 | 9.903 | 12.271 | 32.259 | 20.00 | 20.00 |
| 4 | Paper | redstone | 325 | 325 | 8.255 | 10.309 | 12.828 | 14.636 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 301 | 301 | 0.430 | 0.621 | 0.827 | 1.001 | 20.00 | 20.00 |
| 5 | Chunkwake | cows1000 | 301 | 301 | 0.412 | 0.626 | 0.712 | 1.055 | 20.00 | 20.00 |
| 5 | Chunkwake | redstone | 323 | 323 | 0.386 | 0.579 | 0.686 | 1.126 | 20.03 | 20.00 |
| 5 | Paper | cows200 | 301 | 301 | 2.563 | 3.288 | 4.213 | 5.896 | 20.00 | 19.66 |
| 5 | Paper | cows1000 | 301 | 301 | 8.808 | 11.620 | 34.386 | 115.572 | 19.99 | 20.00 |
| 5 | Paper | redstone | 325 | 325 | 8.603 | 10.752 | 13.564 | 43.066 | 20.01 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1504 | 0.435 | 0.423 | 0.643 | 0.768 | 0.764 | 1.270 | 20.00 | 19.94 | 20.00 |
| cows200 | Paper | 5 | 1506 | 2.520 | 2.450 | 3.287 | 3.957 | 3.617 | 6.359 | 20.02 | 20.00 | 19.72 |
| cows1000 | Chunkwake | 5 | 1505 | 0.413 | 0.397 | 0.623 | 0.786 | 0.761 | 1.146 | 20.00 | 19.99 | 20.00 |
| cows1000 | Paper | 5 | 1506 | 8.571 | 8.297 | 10.596 | 12.949 | 12.151 | 115.572 | 20.02 | 19.99 | 20.00 |
| redstone | Chunkwake | 5 | 1622 | 0.417 | 0.400 | 0.597 | 0.800 | 0.714 | 4.130 | 19.99 | 19.97 | 20.00 |
| redstone | Paper | 5 | 1625 | 8.216 | 8.056 | 9.856 | 12.216 | 11.278 | 45.526 | 20.01 | 20.00 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -3.226 / -3.012 | 5/5 | 0.031 | 1.000 | 0.768 / 3.957 | 0.194x | -0.01 | 1/5 | 0.875 | 0.500 |
| cows1000 | 5 | -15.433 / -11.478 | 5/5 | 0.031 | 1.000 | 0.786 / 12.949 | 0.061x | -0.01 | 1/5 | 0.875 | 0.500 |
| redstone | 5 | -10.689 / -10.229 | 5/5 | 0.031 | 1.000 | 0.800 / 12.216 | 0.066x | -0.02 | 1/5 | 0.969 | 0.188 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
