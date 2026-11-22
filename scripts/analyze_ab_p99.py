#!/usr/bin/env python3
"""Summarize target/ab-p99 produced by scripts/ab_p99.ps1.

Reads PerfProbe summary lines for TPS and raw CSV samples for tick durations.
Writes a Markdown report and prints it.
"""

from __future__ import annotations

import math
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AB = ROOT / "target" / "ab-p99"
SCENARIOS = ("cows200", "cows1000", "redstone")
SIDES = ("Chunkwake", "Paper")

SAMPLE_RE = re.compile(
    r"PERFPROBE SAMPLE label=(?P<label>\S+) n=(?P<n>\d+) ticks=(?P<ticks>\d+) "
    r"wall=(?P<wall>[\d.]+) tps=(?P<tps>[\d.]+)(?: tps_api=(?P<tps_api>[\d.]+))? "
    r"avg=(?P<avg>[\d.]+) p50=(?P<p50>[\d.]+) p95=(?P<p95>[\d.]+) "
    r"p99=(?P<p99>[\d.]+) max=(?P<max>[\d.]+)"
)


@dataclass
class Window:
    round: int
    scenario: str
    side: str
    n: int
    ticks: int
    wall: float
    tps: float
    tps_api: float | None
    samples: list[float]

    def percentile(self, fraction: float) -> float:
        ordered = sorted(self.samples)
        index = max(0, min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1))
        return ordered[index]

    @property
    def avg(self) -> float:
        return sum(self.samples) / len(self.samples)

    @property
    def p50(self) -> float:
        return self.percentile(0.50)

    @property
    def p95(self) -> float:
        return self.percentile(0.95)

    @property
    def p99(self) -> float:
        return self.percentile(0.99)

    @property
    def maximum(self) -> float:
        return max(self.samples)


def parse_log(path: Path) -> dict[str, dict[str, str]]:
    out: dict[str, dict[str, str]] = {}
    if not path.is_file():
        return out
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        match = SAMPLE_RE.search(line)
        if match:
            out[match.group("label")] = match.groupdict()
    return out


def read_samples(path: Path) -> list[float]:
    if not path.is_file():
        return []
    values: list[float] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line or line.startswith("duration_ms"):
            continue
        try:
            values.append(float(line))
        except ValueError:
            pass
    return values


def load() -> list[Window]:
    windows: list[Window] = []
    for round_dir in sorted(AB.glob("r*")):
        try:
            round_number = int(round_dir.name[1:])
        except ValueError:
            continue
        for side in SIDES:
            lower = side.lower()
            log = parse_log(round_dir / f"{lower}.log")
            for scenario in SCENARIOS:
                label = f"r{round_number}-{scenario}"
                summary = log.get(label)
                samples = read_samples(round_dir / f"{lower}-samples" / f"samples-{label}.csv")
                if not summary or not samples:
                    continue
                windows.append(
                    Window(
                        round=round_number,
                        scenario=scenario,
                        side=side,
                        n=len(samples),
                        ticks=int(summary["ticks"]),
                        wall=float(summary["wall"]),
                        tps=float(summary["tps"]),
                        tps_api=float(summary["tps_api"]) if summary.get("tps_api") else None,
                        samples=samples,
                    )
                )
    return windows


def pooled(windows: list[Window]) -> dict[str, float]:
    samples = [value for window in windows for value in window.samples]
    ordered = sorted(samples)
    n = len(ordered)

    def percentile(fraction: float) -> float:
        index = max(0, min(n - 1, math.ceil(n * fraction) - 1))
        return ordered[index]

    total_ticks = sum(window.ticks for window in windows)
    total_wall = sum(window.wall for window in windows)
    tps_values = [window.tps for window in windows]
    api_values = [window.tps_api for window in windows if window.tps_api is not None]
    round_p99 = sorted(window.p99 for window in windows)
    return {
        "rounds": len(windows),
        "n": n,
        "avg": sum(ordered) / n,
        "p50": percentile(0.50),
        "p95": percentile(0.95),
        "p99": percentile(0.99),
        "p99_round_median": round_p99[len(round_p99) // 2],
        "max": ordered[-1],
        "tps": total_ticks / total_wall if total_wall else 0.0,
        "tps_min": min(tps_values),
        "tps_mean": sum(tps_values) / len(tps_values),
        "tps_api": sum(api_values) / len(api_values) if api_values else None,
    }


def sign_test_p(wins: int, losses: int) -> float:
    """One-sided exact sign test: P(X >= wins) under p=0.5.

    Use wins=Chunkwake-better pairs for H1 "Chunkwake better"; swapping
    wins/losses gives the opposite one-sided p-value.
    """
    n = wins + losses
    if n == 0:
        return 1.0
    return sum(math.comb(n, k) for k in range(wins, n + 1)) / (2**n)


def fmt(value: float, digits: int = 3) -> str:
    return f"{value:.{digits}f}"


def main() -> int:
    windows = load()
    if not windows:
        print(f"no complete windows under {AB}", file=sys.stderr)
        return 1

    by_key = {(w.round, w.scenario, w.side): w for w in windows}
    rounds = sorted({w.round for w in windows})
    lines: list[str] = []
    lines.append("# Paper vs Chunkwake: p99 MSPT / TPS")
    lines.append("")
    lines.append(f"Raw data: `{AB.relative_to(ROOT)}`; rounds: {rounds}.")
    lines.append("")
    lines.append("`p99` and all percentiles are computed from raw `ServerTickEndEvent` samples; "
                 "`TPS` is ticks divided by the wall-clock length of the same window.")
    lines.append("")

    lines.append("## Per-round windows")
    lines.append("")
    lines.append("| round | side | scenario | ticks | p50 ms | p95 ms | p99 ms | max ms | TPS (window) | TPS API |")
    lines.append("|---:|---|---|---:|---:|---:|---:|---:|---:|---:|")
    for window in windows:
        api = fmt(window.tps_api, 2) if window.tps_api is not None else "-"
        lines.append(
            f"| {window.round} | {window.side} | {window.scenario} | {window.n} | "
            f"{fmt(window.p50)} | {fmt(window.p95)} | {fmt(window.p99)} | "
            f"{fmt(window.maximum)} | {fmt(window.tps, 2)} | {api} |"
        )
    lines.append("")

    lines.append("## Pooled windows")
    lines.append("")
    lines.append("| scenario | side | rounds | ticks | avg ms | p50 ms | p95 ms | p99 ms | p99 round median | max ms | TPS (pooled window) | TPS min | TPS API mean |")
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for scenario in SCENARIOS:
        for side in SIDES:
            subset = [w for w in windows if w.scenario == scenario and w.side == side]
            if not subset:
                continue
            stats = pooled(subset)
            api = fmt(stats["tps_api"], 2) if stats["tps_api"] is not None else "-"
            lines.append(
                f"| {scenario} | {side} | {stats['rounds']} | {stats['n']} | "
                f"{fmt(stats['avg'])} | {fmt(stats['p50'])} | {fmt(stats['p95'])} | "
                f"{fmt(stats['p99'])} | {fmt(stats['p99_round_median'])} | {fmt(stats['max'])} | "
                f"{fmt(stats['tps'], 2)} | {fmt(stats['tps_min'], 2)} | {api} |"
            )
    lines.append("")

    lines.append("## Paired p99 and TPS deltas (Chunkwake - Paper)")
    lines.append("")
    lines.append("| scenario | pairs | p99 delta ms (mean / median) | p99 CW wins | p99 p(CW better) | p99 p(Paper better) | pooled p99 CW / Paper | p99 ratio | TPS delta (mean) | TPS CW wins | TPS p(CW better) | TPS p(Paper better) |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for scenario in SCENARIOS:
        deltas: list[float] = []
        tps_deltas: list[float] = []
        wins = losses = 0
        tps_wins = tps_losses = 0
        for round_number in rounds:
            cw = by_key.get((round_number, scenario, "Chunkwake"))
            paper = by_key.get((round_number, scenario, "Paper"))
            if not cw or not paper:
                continue
            delta = cw.p99 - paper.p99
            deltas.append(delta)
            if delta < 0:
                wins += 1
            elif delta > 0:
                losses += 1
            tps_delta = cw.tps - paper.tps
            tps_deltas.append(tps_delta)
            if tps_delta > 1e-9:
                tps_wins += 1
            elif tps_delta < -1e-9:
                tps_losses += 1
        if not deltas:
            continue
        cw_pool = pooled([w for w in windows if w.scenario == scenario and w.side == "Chunkwake"])
        paper_pool = pooled([w for w in windows if w.scenario == scenario and w.side == "Paper"])
        mean_delta = sum(deltas) / len(deltas)
        median_delta = sorted(deltas)[len(deltas) // 2]
        mean_tps = sum(tps_deltas) / len(tps_deltas)
        p_cw = sign_test_p(wins, losses)
        p_paper = sign_test_p(losses, wins)
        p_tps_cw = sign_test_p(tps_wins, tps_losses)
        p_tps_paper = sign_test_p(tps_losses, tps_wins)
        ratio = cw_pool["p99"] / paper_pool["p99"] if paper_pool["p99"] else float("nan")
        lines.append(
            f"| {scenario} | {len(deltas)} | {fmt(mean_delta)} / {fmt(median_delta)} | "
            f"{wins}/{len(deltas)} | {fmt(p_cw, 3)} | {fmt(p_paper, 3)} | "
            f"{fmt(cw_pool['p99'])} / {fmt(paper_pool['p99'])} | {fmt(ratio)}x | "
            f"{fmt(mean_tps, 2)} | {tps_wins}/{len(deltas)} | "
            f"{fmt(p_tps_cw, 3)} | {fmt(p_tps_paper, 3)} |"
        )
    lines.append("")
    lines.append("Negative p99 delta means Chunkwake had lower p99. Ties are excluded from the sign test.")
    lines.append("The table gives both one-sided p-values: `p(CW better)` tests "
                 "`Chunkwake < Paper`; `p(Paper better)` tests the opposite direction.")
    lines.append("")
    lines.append("`TPS (window)` is computed from the ticks observed by PerfProbe in that window. "
                 "On Paper this is one event per server tick. On Chunkwake the Java tick queue can "
                 "coalesce events, so its window TPS is a conservative lower bound; `TPS API` comes "
                 "from `Server.getTPS()[0]` (Chunkwake: last-100-tick meter, Paper: 1-minute average).")
    lines.append("")
    lines.append("Chunkwake tick duration = Rust core tick + Java scheduler/plugin tick for the same tick; "
                 "Paper tick duration is the full server tick measured by Paper.")
    lines.append("")

    report = "\n".join(lines)
    print(report)
    out = AB / "results.md"
    out.write_text(report, encoding="utf-8")
    print(f"report: {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
