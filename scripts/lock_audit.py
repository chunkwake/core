#!/usr/bin/env python3
"""Аудит захвата блокировок в тик-пути (Rust).

Ищет `Mutex`/`RwLock`-захваты (`lock()`, `read()`, `write()`) и строит
граф «какая блокировка удерживается, когда берут следующую». Это нужно,
чтобы зафиксировать единый lock-order перед разрезанием тика по
регионам: без него параллельный тик даёт deadlock (проверено живьём).

Запуск:
    python scripts/lock_audit.py                # отчёт в stdout
    python scripts/lock_audit.py --write docs/lock-order.md
"""

from __future__ import annotations

import argparse
import collections
import os
import re
import sys

FILES = [
    "crates/net/src/play.rs",
    "crates/net/src/play/mobs.rs",
    "crates/net/src/play/tracking.rs",
    "crates/net/src/play/stream.rs",
    "crates/net/src/play/state.rs",
    "crates/net/src/play/redstone.rs",
    "crates/net/src/mobs/mod.rs",
]

LOCK_RE = re.compile(r"\.(lock|read|write)\(\)")
DROP_RE = re.compile(r"\bdrop\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)")
FN_RE = re.compile(r"\bfn\s+([A-Za-z0-9_]+)\s*\(")
LET_RE = re.compile(
    r"\blet\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=;]+)?="
)


def strip_code(text: str) -> str:
    """Убирает комментарии и строковые литералы, сохраняя длину и переводы строк."""
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            out.append(" ")
            out.append(" ")
            i += 2
            while i < n and depth:
                if text[i] == "/" and i + 1 < n and text[i + 1] == "*":
                    depth += 1
                    out.append(" ")
                    out.append(" ")
                    i += 2
                elif text[i] == "*" and i + 1 < n and text[i + 1] == "/":
                    depth -= 1
                    out.append(" ")
                    out.append(" ")
                    i += 2
                else:
                    out.append("\n" if text[i] == "\n" else " ")
                    i += 1
            continue
        if c == "r" and i + 1 < n and text[i + 1] in "#\"":
            # raw string r"..." / r#"..."#
            j = i + 1
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                end = text.find('"' + "#" * hashes, j + 1)
                end = n if end < 0 else end + 1 + hashes
                for k in range(i, end):
                    out.append("\n" if text[k] == "\n" else " ")
                i = end
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i, min(j, n)):
                out.append("\n" if text[k] == "\n" else " ")
            i = j
            continue
        if c == "'" and i + 1 < n and (text[i + 1] == "\\" or
                                       (text[i + 2:i + 3] == "'" if i + 2 < n else False)):
            j = i + 1
            while j < n and text[j] != "'":
                j += 2 if text[j] == "\\" else 1
            j = min(j + 1, n)
            for k in range(i, j):
                out.append("\n" if text[k] == "\n" else " ")
            i = j
            continue
        out.append(c)
        i += 1
    return "".join(out)


def lock_key(code: str, dot: int) -> str:
    return receiver(code, dot)


IDENT_END = re.compile(r"[A-Za-z0-9_]$")


def receiver(code: str, dot: int) -> str:
    """Выражение-получатель перед `.lock()`: `self.sim().mobs` и т.п."""
    i = dot - 1
    parts = []
    while i >= 0:
        # пробелы
        while i >= 0 and code[i] in " \t\r\n":
            i -= 1
        if i >= 0 and code[i] == ")":
            # пропускаем аргументы вызова
            depth = 1
            i -= 1
            while i >= 0 and depth:
                if code[i] == ")":
                    depth += 1
                elif code[i] == "(":
                    depth -= 1
                i -= 1
            continue
        if i >= 0 and IDENT_END.match(code[i]):
            j = i
            while j >= 0 and re.match(r"[A-Za-z0-9_]", code[j]):
                j -= 1
            parts.append(code[j + 1:i + 1])
            i = j
            # после идентификатора ожидаем '.'
            while i >= 0 and code[i] in " \t\r\n":
                i -= 1
            if i >= 0 and code[i] == ".":
                i -= 1
                continue
            break
        break
    parts.reverse()
    parts = [p for p in parts if p not in ("self", "mut", "let", "as")]
    if not parts:
        return "?"
    return ".".join(parts[-3:])


def audit_file(path: str):
    text = open(path, "rb").read().decode("utf-8", "surrogateescape")
    code = strip_code(text)
    lines = code.splitlines()

    # Функции: имя по ближайшему `fn` выше позиции.
    fn_starts = []
    for m in FN_RE.finditer(code):
        fn_starts.append((m.start(), m.group(1)))

    def fn_at(pos: int) -> str:
        name = "<top>"
        for start, fname in fn_starts:
            if start <= pos:
                name = fname
            else:
                break
        return name

    # Области видимости: (глубина, имя_переменной, ключ). Поп=при закрытии }.
    guard_stack = []  # list of (depth, var, key)
    depth = 0
    events = []  # (pos, fn, kind, var, key, depth)
    pending_let = None  # (pos, var)
    edges = collections.Counter()
    edge_fns = collections.Counter()

    i = 0
    n = len(code)
    while i < n:
        c = code[i]
        if c == "{":
            depth += 1
            i += 1
            continue
        if c == "}":
            guard_stack = [g for g in guard_stack if g[0] < depth]
            depth -= 1
            pending_let = None
            i += 1
            continue
        m = LET_RE.match(code, i)
        if m:
            pending_let = (m.start(), m.group(1))
            i = m.end()
            continue
        m = DROP_RE.match(code, i)
        if m:
            name = m.group(1)
            guard_stack = [g for g in guard_stack if g[1] != name]
            i = m.end()
            continue
        m = LOCK_RE.match(code, i)
        if m:
            key = lock_key(code, i)
            fn = fn_at(i)
            var = pending_let[1] if pending_let else "?"
            # Всё, что живо снаружи текущей области, — удерживается.
            for (gdepth, gvar, gkey) in guard_stack:
                edges[(gkey, key)] += 1
                edge_fns[(fn, gkey, key)] += 1
            events.append((i, fn, m.group(1), var, key, depth))
            # Временный guard (`expr.lock().unwrap().iter()`) живёт до `;`,
            # а не до `}`; иначе отчёт полон ложных вложений.
            # Guard живёт только если `let x = ...lock().unwrap();` —
            # то есть после `.unwrap()` сразу `;`. Если цепочка продолжается
            # (`let v = ...lock().unwrap().get(..);`), guard временный.
            recent = code[max(0, i - 60):i]
            tail = code[m.end():m.end() + 48]
            guard_alive = bool(re.match(r"\s*(?:;|,\s*\n)", tail))
            if not guard_alive:
                after = re.match(r"\s*\.\s*(?:unwrap|expect)\s*\([^)]*\)", tail)
                if after:
                    rest = tail[after.end():]
                    guard_alive = bool(re.match(r"\s*;", rest))
            ephemeral = "mem::take" in recent or "mem::swap" in recent
            if pending_let is not None and not ephemeral and guard_alive:
                guard_stack.append((depth, var, key))
            pending_let = None
            i = m.end()
            continue
        if pending_let is not None and c == ";":
            pending_let = None
        i += 1

    return events, edges, edge_fns


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", default=None)
    args = ap.parse_args()

    all_events = []
    all_edges = collections.Counter()
    all_edge_fns = collections.Counter()
    per_file = {}
    for path in FILES:
        if not os.path.exists(path):
            continue
        events, edges, edge_fns = audit_file(path)
        per_file[path] = events
        all_events.extend(events)
        all_edges.update(edges)
        all_edge_fns.update(edge_fns)

    order = sorted(all_edges.items())
    locks = collections.Counter()
    for _, _, _, _, key, _ in all_events:
        locks[key] += 1

    cycles = find_cycles(all_edges)

    out = []
    out.append("# Lock-order аудит тик-пути\n")
    out.append("Сгенерировано `scripts/lock_audit.py`.\n")
    out.append(f"Захватов: {len(all_events)}, уникальных блокировок: {len(locks)}.\n")
    out.append("\n## Порядок захвата (наблюдаемые рёбра)\n")
    out.append("| удерживается | берётся | раз |")
    out.append("| --- | --- | ---: |")
    for (a, b), count in order:
        out.append(f"| `{a}` | `{b}` | {count} |")
    out.append("\n## Канонический порядок (предложение)\n")
    canonical = [
        "live", "sleepers", "player_saves", "hub.players",
        "sim.villagers", "sim.mobs_loaded", "sim.mobs",
        "dim_state.edited", "dim_state.chunk_frames", "dim_state.dirty",
        "world_edit",
    ]
    out.append(" -> ".join(f"`{c}`" for c in canonical))
    index = {k: i for i, k in enumerate(canonical)}
    bad = []
    for (fn, a, b), n in all_edge_fns.items():
        if a != b and a in index and b in index and index[a] > index[b]:
            bad.append((a, b, fn, n))
    bad.sort(key=lambda x: -x[3])
    out.append("\n### Нарушения порядка (берут более ранний лок, держа более поздний)\n")
    if bad:
        out.append("| удерживается | берётся | функция | раз |")
        out.append("| --- | --- | --- | ---: |")
        for a, b, fn, n in bad[:40]:
            out.append(f"| `{a}` | `{b}` | `{fn}` | {n} |")
    else:
        out.append("Нарушений нет.\n")
    out.append("\n## Циклы\n")
    if cycles:
        for cycle in cycles:
            out.append("- " + " → ".join(f"`{c}`" for c in cycle))
    else:
        out.append("Циклов в наблюдаемых рёбрах нет.\n")
    out.append("\n## По файлам\n")
    for path, events in per_file.items():
        out.append(f"\n### `{path}` ({len(events)})\n")
        seen = []
        for _, fn, kind, var, key, _ in events:
            line = f"- `{fn}`: `{key}.{kind}()` → `{var}`"
            if line not in seen:
                seen.append(line)
        out.extend(seen)
    text = "\n".join(out) + "\n"
    if args.write:
        open(args.write, "w", encoding="utf-8", newline="\n").write(text)
        print(f"written {args.write}")
    else:
        sys.stdout.write(text)
    return 0


def find_cycles(edges):
    """Простые циклы длиной 2-4 для отчёта."""
    adj = collections.defaultdict(set)
    for (a, b) in edges:
        adj[a].add(b)
    found = []
    nodes = list(adj)

    def walk(start, node, path):
        if len(path) > 5 or len(found) > 20:
            return
        for nxt in adj.get(node, ()):
            if nxt == start and len(path) >= 2:
                cyc = path[:]
                key = tuple(sorted(cyc))
                if key not in {tuple(sorted(c)) for c in found}:
                    found.append(cyc)
            elif nxt not in path:
                walk(start, nxt, path + [nxt])

    for n in nodes:
        walk(n, n, [n])
    return found


if __name__ == "__main__":
    raise SystemExit(main())
