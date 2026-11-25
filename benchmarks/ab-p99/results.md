# Paper vs Chunkwake: p99 MSPT / TPS

Raw data: `target\ab-p99-plugin`; rounds: [1, 2, 3, 4, 5].

`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; `TPS` uses `server_ticks` (Server.getCurrentTick delta) divided by wall-clock length of the same window.

## Per-round windows

| round | side | scenario | events | server ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Chunkwake | cows200 | 300 | 300 | 0.526 | 0.770 | 0.881 | 2.081 | 20.00 | 20.00 |
| 1 | Chunkwake | cows1000 | 300 | 300 | 1.112 | 1.458 | 1.610 | 1.769 | 20.00 | 20.00 |
| 1 | Chunkwake | redstone | 300 | 300 | 1.221 | 1.606 | 1.913 | 2.202 | 20.00 | 20.00 |
| 1 | Paper | cows200 | 300 | 300 | 3.836 | 5.574 | 22.330 | 44.462 | 20.00 | 19.88 |
| 1 | Paper | cows1000 | 300 | 300 | 10.012 | 11.540 | 12.969 | 32.705 | 20.00 | 20.00 |
| 1 | Paper | redstone | 300 | 300 | 9.582 | 11.446 | 13.517 | 41.563 | 20.00 | 20.00 |
| 2 | Chunkwake | cows200 | 300 | 300 | 0.629 | 0.862 | 0.981 | 2.563 | 19.99 | 20.00 |
| 2 | Chunkwake | cows1000 | 300 | 300 | 1.239 | 1.604 | 1.995 | 2.843 | 20.00 | 20.00 |
| 2 | Chunkwake | redstone | 300 | 300 | 1.318 | 1.811 | 2.001 | 2.657 | 20.01 | 20.00 |
| 2 | Paper | cows200 | 300 | 300 | 3.603 | 5.278 | 30.576 | 105.285 | 20.00 | 19.81 |
| 2 | Paper | cows1000 | 300 | 300 | 10.212 | 12.200 | 14.383 | 30.350 | 20.00 | 20.00 |
| 2 | Paper | redstone | 300 | 300 | 9.765 | 11.771 | 13.844 | 43.244 | 20.00 | 20.00 |
| 3 | Chunkwake | cows200 | 300 | 300 | 0.616 | 0.922 | 1.124 | 2.963 | 20.00 | 20.00 |
| 3 | Chunkwake | cows1000 | 300 | 300 | 1.254 | 1.649 | 1.915 | 2.928 | 20.00 | 20.00 |
| 3 | Chunkwake | redstone | 300 | 300 | 1.278 | 1.620 | 1.831 | 2.691 | 20.00 | 20.00 |
| 3 | Paper | cows200 | 300 | 300 | 3.743 | 5.209 | 73.237 | 153.763 | 20.00 | 19.64 |
| 3 | Paper | cows1000 | 300 | 300 | 9.950 | 11.895 | 12.947 | 38.654 | 20.00 | 20.00 |
| 3 | Paper | redstone | 300 | 300 | 9.489 | 11.426 | 13.461 | 40.166 | 20.00 | 20.00 |
| 4 | Chunkwake | cows200 | 300 | 300 | 0.555 | 0.760 | 0.983 | 2.155 | 20.00 | 20.00 |
| 4 | Chunkwake | cows1000 | 300 | 300 | 1.050 | 1.349 | 1.504 | 2.273 | 20.00 | 20.00 |
| 4 | Chunkwake | redstone | 300 | 300 | 1.073 | 1.325 | 1.720 | 1.933 | 19.41 | 19.80 |
| 4 | Paper | cows200 | 300 | 300 | 3.476 | 7.318 | 52.587 | 251.907 | 20.00 | 19.84 |
| 4 | Paper | cows1000 | 300 | 300 | 9.937 | 11.232 | 13.019 | 38.259 | 20.00 | 20.00 |
| 4 | Paper | redstone | 300 | 300 | 7.880 | 9.685 | 10.652 | 35.610 | 20.00 | 20.00 |
| 5 | Chunkwake | cows200 | 300 | 300 | 0.604 | 0.834 | 0.969 | 2.189 | 20.01 | 20.00 |
| 5 | Chunkwake | cows1000 | 300 | 300 | 1.220 | 1.588 | 1.820 | 2.161 | 19.99 | 20.00 |
| 5 | Chunkwake | redstone | 300 | 300 | 1.220 | 1.601 | 1.931 | 2.919 | 20.01 | 20.00 |
| 5 | Paper | cows200 | 300 | 300 | 4.052 | 6.026 | 66.201 | 78.247 | 20.02 | 19.77 |
| 5 | Paper | cows1000 | 300 | 300 | 9.852 | 11.561 | 12.728 | 34.914 | 20.00 | 20.00 |
| 5 | Paper | redstone | 300 | 300 | 9.677 | 12.767 | 15.857 | 39.722 | 20.00 | 20.00 |

## Pooled windows

| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | Chunkwake | 5 | 1500 | 0.615 | 0.584 | 0.854 | 1.000 | 0.981 | 2.963 | 20.00 | 19.99 | 20.00 |
| cows200 | Paper | 5 | 1500 | 5.400 | 3.747 | 5.899 | 51.417 | 52.587 | 251.907 | 20.00 | 20.00 | 19.79 |
| cows1000 | Chunkwake | 5 | 1500 | 1.199 | 1.167 | 1.573 | 1.843 | 1.820 | 2.928 | 20.00 | 19.99 | 20.00 |
| cows1000 | Paper | 5 | 1500 | 10.211 | 9.992 | 11.807 | 13.203 | 12.969 | 38.654 | 20.00 | 20.00 | 20.00 |
| redstone | Chunkwake | 5 | 1500 | 1.248 | 1.219 | 1.663 | 1.931 | 1.913 | 2.919 | 19.88 | 19.41 | 19.96 |
| redstone | Paper | 5 | 1500 | 9.632 | 9.461 | 11.748 | 14.037 | 13.517 | 43.244 | 20.00 | 20.00 | 20.00 |

## Paired p99 and TPS deltas (Chunkwake - Paper)

| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cows200 | 5 | -47.998 / -51.604 | 5/5 | 0.031 | 1.000 | 1.000 / 51.417 | 0.019x | -0.00 | 0/5 | 1.000 | 0.250 |
| cows1000 | 5 | -11.440 / -11.359 | 5/5 | 0.031 | 1.000 | 1.843 / 13.203 | 0.140x | -0.00 | 0/5 | 1.000 | 0.500 |
| redstone | 5 | -11.587 / -11.630 | 5/5 | 0.031 | 1.000 | 1.931 / 14.037 | 0.138x | -0.11 | 2/5 | 0.500 | 0.875 |

Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.
The table gives both one-sided p-values: `p(CW better)` tests `Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.

`TPS (window)` uses the authoritative `server_ticks` delta from `Server.getCurrentTick()`, not the number of Java tick events. `TPS API` from `Server.getTPS()[0]` is shown as a cross-check (Chunkwake: last-100-tick meter, Paper: 1-minute average).

Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; Paper tick duration is the full server tick measured by Paper.
