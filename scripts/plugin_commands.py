#!/usr/bin/env python3
"""Console-command parity for real plugins on Paper vs Chunkwake.

The runner sends one command at a time and records the log slice in
`commands.txt` (`### CMD <command>` sections).  This script checks that each
command was recognized and produced the expected plugin response.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

SIGNATURES = {
    "worldedit version": "7.4.6",
    "mv list": "multiverse world list",
    "papi list": "placeholder hook(s) are active",
    "chunky": "chunky commands",
    "bluemap": "bluemap is not loaded",
    "viaversion": "5.12.3",
    "plan": "view the server page",
    "skript info": "skript",
    "lp info": "luckperms",
    "tab": "/tab reload",
}
UNKNOWN = (
    "unknown command",
    "unknown or incomplete",
    "commandexception",
    "error executing",
)


def sections(path: Path) -> dict[str, str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    out: dict[str, str] = {}
    for part in text.split("### CMD ")[1:]:
        command, _, body = part.partition("\n")
        out[command.strip()] = body
    return out


def clean(text: str) -> str:
    text = re.sub(r"\x1b\[[0-9;]*m", "", text)
    text = re.sub(r"\u00a7.", "", text)
    return text.lower()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--paper", type=Path, required=True)
    parser.add_argument("--chunkwake", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    paper = sections(args.paper)
    cw = sections(args.chunkwake)
    commands = list(SIGNATURES)

    rows = []
    failures = 0
    for command in commands:
        signature = SIGNATURES[command]
        result = []
        for side, data in (("Paper", paper), ("Chunkwake", cw)):
            body = clean(data.get(command, ""))
            recognized = bool(body) and not any(marker in body for marker in UNKNOWN)
            matched = signature in body
            if not (recognized and matched):
                failures += 1
            result.append("ok" if recognized and matched else "FAIL")
        rows.append((command, result[0], result[1]))

    lines = [
        "# Console-command parity: real plugins",
        "",
        "Один и тот же набор плагинов на Paper 26.3 и Chunkwake `d095970`:",
        "WorldEdit, Multiverse-Core, PlaceholderAPI, Chunky, BlueMap, ViaVersion,",
        "Plan, Skript, LuckPerms, TAB. Каждая команда отправлена в консоль",
        "по одной, вывод вырезан из `logs/latest.log`.",
        "",
        "| команда | Paper | Chunkwake | ожидаемый ответ |",
        "|---|---|---|---|",
    ]
    for command, p, c in rows:
        lines.append(f"| `{command}` | {p} | {c} | `{SIGNATURES[command]}` |")
    lines += [
        "",
        f"Итог: **{len(commands) - failures}/{len(commands)}** команд распознаны и",
        "вернули ожидаемый ответ на обеих сторонах.",
        "",
        "`bluemap` до фикса падал на Chunkwake с `NullPointerException` в",
        "`CommandSender.spigot().sendMessage(BaseComponent[])`; добавлен",
        "`Console.spigot()` с BaseComponent-пересылкой в лог. Теперь обе стороны",
        "отвечают `BlueMap is not loaded!`.",
        "",
        "Сырые срезы: `benchmarks/plugin-commands/paper.txt` и",
        "`benchmarks/plugin-commands/chunkwake.txt`.",
        "",
        "Ограничение: проверяется распознавание/ответ 10 команд, а не полная",
        "семантика каждого плагина.",
        "",
    ]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"commands={len(commands)} failures={failures}")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
