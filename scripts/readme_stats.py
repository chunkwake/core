#!/usr/bin/env python3
"""Синхронизирует счётчики механик в README и превью с фич-матрицей.

Единственный источник данных — docs/feature-matrix.md. Скрипт считает
строки со статусами «готово», «частично» и «нет» и обновляет:

  * три бейджа и два предложения в README.md;
  * числа в docs/assets/preview-ru.svg.

Запуск:
    python scripts/readme_stats.py          # обновить
    python scripts/readme_stats.py --check  # проверить (для CI)
    python scripts/readme_stats.py --stage  # обновить и git add (для хука)
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs" / "feature-matrix.md"
README = ROOT / "README.md"
PREVIEW = ROOT / "docs" / "assets" / "preview-ru.svg"

# Статус в матрице -> ключ счётчика.
STATUSES = {"готово": "ready", "частично": "partial", "нет": "none"}


def read_counts() -> dict[str, int]:
    """Считает строки только тех таблиц, у которых второй столбец — «Статус»."""
    counts = {key: 0 for key in STATUSES.values()}
    block: list[tuple[int, str]] = []

    def count_block(rows: list[tuple[int, str]]) -> None:
        if len(rows) < 3:
            return
        header = [cell.strip() for cell in rows[0][1].strip().strip("|").split("|")]
        if len(header) < 2 or header[1] != "Статус":
            return
        for number, line in rows[2:]:
            cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
            if len(cells) < 2:
                continue
            status = cells[1]
            key = STATUSES.get(status)
            if key is None:
                raise SystemExit(
                    f"{MATRIX}:{number}: неизвестный статус {status!r}; "
                    "добавьте его в STATUSES в scripts/readme_stats.py"
                )
            counts[key] += 1

    for number, line in enumerate(MATRIX.read_text(encoding="utf-8").splitlines(), 1):
        if line.lstrip().startswith("|"):
            block.append((number, line))
        else:
            count_block(block)
            block = []
    count_block(block)

    if not any(counts.values()):
        raise SystemExit(f"{MATRIX}: не найдено ни одной таблицы со статусами")
    return counts


def replace_once(text: str, pattern: str, replacement: str, where: str) -> str:
    new, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise SystemExit(
            f"{README}: не найден шаблон для {where}. "
            "Поправьте scripts/readme_stats.py или верните прежний текст."
        )
    return new


def update_readme(text: str, counts: dict[str, int]) -> str:
    for alt, key in (("готово", "ready"), ("частично", "partial"), ("не начато", "none")):
        pattern = (
            rf"(!\[{re.escape(alt)}\]"
            r"\(https://img\.shields\.io/badge/[^)\s]*-)\d+(%20[^)]*\))"
        )
        text = replace_once(text, pattern, rf"\g<1>{counts[key]}\g<2>", f"бейджа «{alt}»")

    ru = (
        r"(По \[фич-матрице\]\(docs/feature-matrix\.md\): \*\*)\d+"
        r"(\*\* семейств механик готово,\n\*\*)\d+"
        r"(\*\* частично, \*\*)\d+"
        r"(\*\* не начато\.)"
    )
    text = replace_once(
        text,
        ru,
        rf"\g<1>{counts['ready']}\g<2>{counts['partial']}\g<3>{counts['none']}\g<4>",
        "русского предложения со счётчиками",
    )

    en = (
        r"(see the \[feature matrix\]\(docs/feature-matrix\.md\) \()\d+"
        r"( done,\n  )\d+"
        r"( partial, )\d+"
        r"( not started\)\.)"
    )
    text = replace_once(
        text,
        en,
        rf"\g<1>{counts['ready']}\g<2>{counts['partial']}\g<3>{counts['none']}\g<4>",
        "английского предложения со счётчиками",
    )
    return text


def update_preview(text: str, counts: dict[str, int]) -> str:
    for key, value in counts.items():
        pattern = rf'(<text id="stat-{key}"[^>]*>)\d+(</text>)'
        new, count = re.subn(pattern, rf"\g<1>{value}\g<2>", text, count=1)
        if count != 1:
            raise SystemExit(
                f"{PREVIEW}: не найден элемент stat-{key}. "
                f'Превью должно содержать <text id="stat-{key}">.'
            )
        text = new
    return text


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--check", action="store_true", help="только проверить, ничего не менять")
    group.add_argument("--stage", action="store_true", help="после обновления выполнить git add")
    args = parser.parse_args()

    counts = read_counts()
    readme_old = README.read_text(encoding="utf-8")
    preview_old = PREVIEW.read_text(encoding="utf-8")
    readme_new = update_readme(readme_old, counts)
    preview_new = update_preview(preview_old, counts)

    changed = []
    if readme_new != readme_old:
        changed.append(README)
    if preview_new != preview_old:
        changed.append(PREVIEW)

    summary = (
        f"{counts['ready']} готово, {counts['partial']} частично, {counts['none']} нет"
    )

    if args.check:
        if changed:
            for path in changed:
                print(f"устарел: {path.relative_to(ROOT)}", file=sys.stderr)
            print("запустите: python scripts/readme_stats.py", file=sys.stderr)
            return 1
        print(f"счётчики актуальны: {summary}")
        return 0

    if readme_new != readme_old:
        README.write_text(readme_new, encoding="utf-8", newline="\n")
        print(f"обновлён: {README.relative_to(ROOT)}")
    if preview_new != preview_old:
        PREVIEW.write_text(preview_new, encoding="utf-8", newline="\n")
        print(f"обновлён: {PREVIEW.relative_to(ROOT)}")

    if args.stage and changed:
        subprocess.run(
            ["git", "add", "--", *(str(path.relative_to(ROOT)) for path in changed)],
            cwd=ROOT,
            check=True,
        )

    print(f"счётчики: {summary}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
