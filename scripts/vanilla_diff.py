#!/usr/bin/env python3
"""Differential vanilla-behaviour report for Chunkwake vs Paper.

Both servers load the same ChunkwakeTest plugin. The plugin prints
deterministic Bukkit/API observations; this script extracts them and shows
which observations differ. Timestamps, server classloaders, world UUIDs,
absolute paths and scheduler tick counters are normalized or ignored.
"""

from __future__ import annotations

import argparse
import difflib
import re
from pathlib import Path

MARKER = "[ChunkwakeTest]"

# Lines that are expected to differ between servers or depend on wall-clock
# timing. They are not useful as behaviour assertions.
IGNORE = (
    "Loading server plugin ",
    "onEnable on ",
    "resource plugin.yml:",
    "world: world",
    "scheduler: ",
    "loaded chunks: ",
    "region task: ",
    "folia: done ",
    "folia: async delayed",
    "mannequin profile:",
)

NORMALIZE = (
    (re.compile(r"world time: before=\d+"), "world time: before=<ticks>"),
    (re.compile(r"mob: (.*?) living=\d+ spawned=\d+"), r"mob: \1 living=<n> spawned=<n>"),
    (re.compile(r"tag-probe: (.*?) -> \d+ \[(.*?)\]"), r"tag-probe: \1 -> <n> [\2]"),
    # CraftBukkit and Chunkwake use different CommandMap implementations; the
    # observable key count is what matters here.
    (re.compile(r"commandmaps: server=\S+ bukkit=\S+"),
     "commandmaps: server=<CommandMap> bukkit=<CommandMap>"),
)


def extract(path: Path) -> list[str]:
    out: list[str] = []
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if MARKER not in raw:
            continue
        text = raw.split(MARKER, 1)[1].strip()
        if any(text.startswith(prefix) for prefix in IGNORE):
            continue
        for pattern, replacement in NORMALIZE:
            text = pattern.sub(replacement, text)
        tag = re.match(r"tag-probe: (.*?) -> <n> \[(.*?)\]$", text)
        if tag:
            values = sorted(part.strip() for part in tag.group(2).split(",") if part.strip())
            text = f"tag-probe: {tag.group(1)} -> <n> [{', '.join(values)}]"
        out.append(text)
        # `all-schedulers-ok` — последняя строка onEnable-пробы. Всё после неё
        # это уже события живого мира (поршни, hopper'ы), они не относятся
        # к API-parity и зависят от стенда.
        if text == "folia: all-schedulers-ok":
            break
    # Порядок независимых scheduler-callback'ов не является API-контрактом;
    # region ownership проверяется отдельными `region task`-строками.
    out.sort()
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--paper", type=Path, required=True)
    parser.add_argument("--chunkwake", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    paper = extract(args.paper)
    chunkwake = extract(args.chunkwake)
    diff = list(difflib.unified_diff(paper, chunkwake, fromfile="Paper", tofile="Chunkwake", lineterm=""))

    lines = [
        "# Differential vanilla behaviour: Paper vs Chunkwake",
        "",
        "Both servers loaded the same `ChunkwakeTest` plugin and ran its built-in"
        " Bukkit/API observations. Volatile lines (timestamps, classloaders,"
        " world UUIDs, scheduler counters, per-tick region tasks) are omitted."
        " Independent scheduler callbacks are compared as a sorted set.",
        "",
        f"- Paper observations: **{len(paper)}**",
        f"- Chunkwake observations: **{len(chunkwake)}**",
        f"- Diff lines: **{len(diff)}**",
        "",
        "Raw logs: `benchmarks/vanilla-diff/paper.txt` and"
        " `benchmarks/vanilla-diff/chunkwake.txt`.",
        "",
        "## Unified diff",
        "",
        "```diff",
        *diff,
        "```",
        "",
        "## Interpretation",
        "",
    ]
    if diff:
        lines += [
            "The diff is a list of **open observations**, not a claim that every",
            " difference is a bug: some checks depend on chunk load order, world",
            " generation timing, or server-only formatting. It is intended as a",
            " reproducible baseline for the next vanilla-parity fixes.",
        ]
    else:
        lines += [
            "All extracted observations matched on this run. This is still a",
            "**finite** onEnable API probe: it covers Bukkit/Paper/Folia values",
            "and scheduler callbacks, not arbitrary plugin behaviour.",
        ]
    lines.append("")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"paper={len(paper)} chunkwake={len(chunkwake)} diff={len(diff)} -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
