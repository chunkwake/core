"""Статистика A/B «чанки против 8x8-регионов» по логам scripts/ab-regions.ps1.

Читает `load_<mode>_<round>.log` (region_load печатает `MSPT <ms>`),
считает p50/p95/mean по каждому прогону, затем парные дельты
(regions - chunks) и общую сводку.

Запуск: py -3 scripts/ab-regions-stats.py <каталог с load_*.log>
"""

import glob
import os
import re
import statistics
import sys


def percentile(values: list[float], q: float) -> float:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, int(q * len(ordered) + 0.999999) - 1))
    return ordered[index]


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: ab-regions-stats.py <dir>", file=sys.stderr)
        return 2
    logs = sys.argv[1]
    runs: dict[tuple[str, int], list[float]] = {}
    for path in sorted(glob.glob(os.path.join(logs, "load_*.log"))):
        match = re.match(r"load_(chunks|regions)_(\d+)\.log", os.path.basename(path))
        if not match:
            continue
        mode, round_no = match.group(1), int(match.group(2))
        text = open(path, encoding="utf-8", errors="replace").read()
        runs[(mode, round_no)] = [float(v) for v in re.findall(r"^MSPT ([0-9.]+)", text, re.M)]

    if not runs:
        print("no load_*.log with MSPT samples", file=sys.stderr)
        return 1

    print(f"{'run':10} {'n':>3} {'p50':>7} {'p95':>7} {'mean':>7} {'max':>7}")
    for (mode, round_no), values in sorted(runs.items(), key=lambda kv: (kv[0][1], kv[0][0])):
        if not values:
            continue
        print(
            f"{mode + str(round_no):10} {len(values):3d} "
            f"{percentile(values, 0.50):7.3f} {percentile(values, 0.95):7.3f} "
            f"{statistics.mean(values):7.3f} {max(values):7.3f}"
        )

    rounds = sorted({r for (_, r) in runs})
    deltas = []
    print()
    for round_no in rounds:
        chunks = runs.get(("chunks", round_no))
        regions = runs.get(("regions", round_no))
        if not chunks or not regions:
            continue
        c50, r50 = percentile(chunks, 0.50), percentile(regions, 0.50)
        deltas.append(r50 - c50)
        print(
            f"pair {round_no}: chunks p50={c50:.3f} regions p50={r50:.3f} "
            f"delta={r50 - c50:+.3f} ms"
        )

    if deltas:
        positives = sum(1 for value in deltas if value > 0)
        print()
        print(
            f"delta median {statistics.median(deltas):+.3f} ms, "
            f"mean {statistics.mean(deltas):+.3f} ms, "
            f"stdev {statistics.stdev(deltas):.3f} ms, "
            f"min {min(deltas):+.3f}, max {max(deltas):+.3f}, "
            f"positive_pairs {positives}/{len(deltas)}"
        )
        for mode in ("chunks", "regions"):
            pooled = [v for (m, _), values in runs.items() if m == mode for v in values]
            if pooled:
                print(
                    f"{mode:8} pooled p50={percentile(pooled, 0.50):.3f} "
                    f"p95={percentile(pooled, 0.95):.3f} "
                    f"p99={percentile(pooled, 0.99):.3f} n={len(pooled)}"
                )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
