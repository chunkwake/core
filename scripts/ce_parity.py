#!/usr/bin/env python3
"""CraftEngine custom-block parity for Paper vs Chunkwake.

Both servers load the same `CEProbe`/`CEStateProbe` and CraftEngine.  The
probes place custom blocks, read them back, break them and report BlockData,
custom state, collision boxes and drops.  This script extracts the stable
observations and prints mismatches.

Unstable lines are excluded: dropped item entities (world leftovers), object
identity hashes, the generated proxy class name and the huge reflection dumps.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

SKIP = (
    "CEProbe: Loading",
    "CEProbe: Enabling",
    "CEStateProbe: Loading",
    "CEStateProbe: Enabling",
    "CEPROBE state2",
    "CEPROBE block-",
    "CEPROBE nms-",
    "CEPROBE world-",
    "CEPROBE loaded-blocks",
    "CEPROBE BR-",
    "CEPROBE place-first",
    "dropped-entities=",
    "CESTATEPROBE default=",
    "CESTATEPROBE desired=",
    "CESTATEPROBE props=",
)


def extract(path: Path) -> list[str]:
    out: list[str] = []
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        match = re.search(r"\[(CEProbe|CEStateProbe)\]\s*(.*)", raw)
        if not match:
            continue
        text = match.group(1) + ": " + match.group(2).strip()
        if any(model in text for model in SKIP):
            continue
        text = re.sub(r"craftengine:custom_\d+", "craftengine:custom_N", text)
        text = re.sub(r"@[0-9a-f]{4,}", "@hash", text)
        text = re.sub(r"jdk\.proxy\d+\.\$Proxy\d+", "<BlockData>", text)
        text = re.sub(
            r"org\.bukkit\.craftbukkit\.block\.data\.CraftBlockData",
            "<BlockData>",
            text,
        )
        out.append(text)
    out.sort()
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--paper", type=Path, required=True)
    parser.add_argument("--chunkwake", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    paper = extract(args.paper)
    cw = extract(args.chunkwake)
    only_p = [line for line in paper if line not in cw]
    only_c = [line for line in cw if line not in paper]

    lines = [
        "# CraftEngine parity: Paper vs Chunkwake",
        "",
        "Один и тот же `CEProbe`/`CEStateProbe` и CraftEngine 26.9.2 на Paper 26.3",
        "и Chunkwake `d095970`, чистые `plugins/CEProbe` (без старого `placed.flag`).",
        "Сравниваются custom-блоки `default:topaz_ore`, `table_lamp`, `palm_stairs`,",
        "`palm_door`: `BlockData`, state, collision, drops, persist/fresh. Исключены",
        "отладочные dump-строки, identity hash, сгенерированный proxy-класс и",
        "`place-first` (зависит от того, стоял ли блок в мире до прогона).",
        "",
        f"- Paper observations: **{len(paper)}**",
        f"- Chunkwake observations: **{len(cw)}**",
        f"- Mismatches: **{len(only_p) + len(only_c)}**",
        "",
        "## Mismatches",
        "",
    ]
    if only_p or only_c:
        lines += ["```diff", "--- Paper only", "+++ Chunkwake only"]
        lines += ["- " + line for line in only_p]
        lines += ["+ " + line for line in only_c]
        lines += ["```"]
    else:
        lines += ["Нет: все ключевые наблюдения совпали после нормализации."]
    lines += [
        "",
        "## Совпавшие наблюдения",
        "",
        "```text",
        *paper,
        "```",
        "",
        "Сырые логи: `benchmarks/ce-parity/paper.txt` и",
        "`benchmarks/ce-parity/chunkwake.txt`.",
        "",
    ]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"paper={len(paper)} chunkwake={len(cw)} mismatch={len(only_p) + len(only_c)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
