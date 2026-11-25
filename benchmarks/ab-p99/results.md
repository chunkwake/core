# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99-plugin`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 300 | 300 | 0.657 | 0.953 | 1.068 | 2.385 | 20.00 | 20.00 |
| 1 | Chunkwake | cows1000 | 300 | 300 | 1.394 | 1.790 | 2.360 | 2.695 | 20.00 | 20.00 |
| 1 | Chunkwake | redstone | 300 | 300 | 1.401 | 1.858 | 2.164 | 2.660 | 20.00 | 20.00 |
| 1 | Paper | cows200 | 300 | 300 | 4.058 | 7.064 | 37.123 | 74.857 | 20.01 | 19.60 |
| 1 | Paper | cows1000 | 300 | 300 | 10.383 | 24.919 | 46.250 | 210.524 | 19.87 | 19.97 |
| 1 | Paper | redstone | 300 | 300 | 10.889 | 12.971 | 13.956 | 42.684 | 19.99 | 19.96 |
| 2 | Chunkwake | cows200 | 300 | 300 | 0.556 | 0.781 | 0.919 | 2.119 | 20.00 | 20.00 |
| 2 | Chunkwake | cows1000 | 300 | 300 | 1.103 | 1.508 | 2.121 | 2.446 | 20.00 | 20.00 |
| 2 | Chunkwake | redstone | 300 | 300 | 1.197 | 1.586 | 1.803 | 2.438 | 20.00 | 20.00 |
| 2 | Paper | cows200 | 300 | 300 | 3.486 | 6.083 | 51.923 | 228.978 | 20.00 | 19.46 |
| 2 | Paper | cows1000 | 300 | 300 | 9.174 | 10.682 | 12.694 | 126.877 | 20.00 | 20.00 |
| 2 | Paper | redstone | 300 | 300 | 8.807 | 10.900 | 12.386 | 42.658 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 300 | 300 | 0.535 | 0.795 | 0.944 | 2.782 | 20.00 | 20.00 |
| 3 | Chunkwake | cows1000 | 300 | 300 | 1.169 | 1.580 | 1.827 | 3.259 | 20.00 | 20.00 |
| 3 | Chunkwake | redstone | 300 | 300 | 1.264 | 1.786 | 2.445 | 4.132 | 20.00 | 20.00 |
| 3 | Paper | cows200 | 300 | 300 | 4.271 | 13.567 | 89.672 | 124.722 | 20.00 | 19.76 |
| 3 | Paper | cows1000 | 300 | 300 | 12.135 | 14.909 | 18.026 | 33.957 | 20.00 | 20.00 |
| 3 | Paper | redstone | 300 | 300 | 11.307 | 14.431 | 17.038 | 53.067 | 19.99 | 20.00 |
| 4 | Chunkwake | cows200 | 300 | 300 | 0.639 | 0.905 | 1.139 | 2.545 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 300 | 300 | 1.293 | 1.775 | 2.030 | 2.542 | 19.99 | 20.00 |
| 4 | Chunkwake | redstone | 300 | 300 | 1.453 | 2.389 | 5.220 | 18.256 | 19.99 | 20.00 |
| 4 | Paper | cows200 | 300 | 300 | 4.396 | 7.348 | 52.243 | 146.312 | 20.00 | 19.74 |
| 4 | Paper | cows1000 | 300 | 300 | 11.348 | 13.494 | 14.354 | 45.285 | 20.01 | 20.00 |
| 4 | Paper | redstone | 300 | 300 | 10.589 | 12.484 | 13.615 | 29.057 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 300 | 300 | 0.664 | 0.912 | 1.075 | 3.462 | 19.99 | 20.00 |
| 5 | Chunkwake | cows1000 | 300 | 300 | 1.268 | 1.614 | 1.773 | 2.753 | 20.01 | 20.00 |
| 5 | Chunkwake | redstone | 300 | 300 | 1.488 | 2.031 | 2.544 | 3.279 | 20.00 | 20.00 |
| 5 | Paper | cows200 | 300 | 300 | 3.822 | 5.322 | 33.485 | 150.637 | 20.02 | 19.44 |
| 5 | Paper | cows1000 | 300 | 300 | 11.142 | 12.734 | 14.375 | 42.186 | 20.01 | 20.00 |
| 5 | Paper | redstone | 300 | 300 | 10.842 | 12.437 | 14.123 | 43.643 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1500 | 0.648 | 0.612 | 0.887 | 1.066 | 1.068 | 3.462 | 20.00 | 19.99 | 20.00 |
| cows200 | Paper | 5 | 1500 | 5.910 | 4.021 | 7.228 | 58.294 | 51.923 | 228.978 | 20.01 | 20.00 | 19.60 |
| cows1000 | Chunkwake | 5 | 1500 | 1.288 | 1.242 | 1.694 | 2.121 | 2.030 | 3.259 | 20.00 | 19.99 | 20.00 |
| cows1000 | Paper | 5 | 1500 | 11.754 | 10.974 | 14.171 | 33.957 | 14.375 | 210.524 | 19.98 | 19.87 | 19.99 |
| redstone | Chunkwake | 5 | 1500 | 1.446 | 1.357 | 1.955 | 2.940 | 2.445 | 18.256 | 20.00 | 19.99 | 20.00 |
| redstone | Paper | 5 | 1500 | 10.763 | 10.584 | 13.084 | 14.995 | 13.956 | 53.067 | 20.00 | 19.99 | 19.99 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -51.860 / -51.003 | 5/5 | 0.031 | 1.000 | 1.066 / 58.294 | 0.018x | -0.01 | 0/5 | 1.000 | 0.250 |
| cows1000 | 5 | -19.118 / -12.602 | 5/5 | 0.031 | 1.000 | 2.121 / 33.957 | 0.062x | 0.02 | 1/5 | 0.750 | 0.750 |
| redstone | 5 | -11.388 / -11.579 | 5/5 | 0.031 | 1.000 | 2.940 / 14.995 | 0.196x | 0.00 | 2/5 | 0.500 | 0.875 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
