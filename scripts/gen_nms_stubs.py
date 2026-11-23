#!/usr/bin/env python3
"""Generate missing net.minecraft.* stubs from Paper class metadata.

This is the generator half of scripts/scan_plugin_nms.py. It is deliberately
conservative: only public/protected top-level classes that really exist in the
Paper server jar are generated, private members are skipped, and unknown nested
types fall back to Object.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from scan_plugin_nms import existing_sources, scan_jar  # noqa: E402


def find_javap() -> str:
    javap = shutil.which("javap")
    if javap:
        return javap
    home = Path.home()
    for jdk in sorted((home / ".gradle" / "jdks").glob("*25*")):
        candidate = jdk / "bin" / "javap.exe"
        if candidate.is_file():
            return str(candidate)
        candidate = jdk / "bin" / "javap"
        if candidate.is_file():
            return str(candidate)
    raise SystemExit("javap not found")


def descriptor_type(desc: str, pos: int, known: set[str]) -> tuple[str, int]:
    if desc[pos] == "[":
        inner, pos = descriptor_type(desc, pos + 1, known)
        return inner + "[]", pos
    if desc[pos] == "L":
        end = desc.index(";", pos)
        name = desc[pos + 1:end].replace("/", ".")
        if name.startswith("net.minecraft.") and name not in known \
                and name.replace(".", "/") not in known:
            name = "java.lang.Object"
        return name, end + 1
    primitive = {
        "V": "void",
        "Z": "boolean",
        "B": "byte",
        "C": "char",
        "S": "short",
        "I": "int",
        "J": "long",
        "F": "float",
        "D": "double",
    }[desc[pos]]
    return primitive, pos + 1


def default_value(type_name: str) -> str:
    if type_name == "void":
        return ""
    if type_name in ("boolean",):
        return "false"
    if type_name in ("byte", "char", "short", "int", "long", "float", "double"):
        return "0"
    return "null"


def parse_members(output: str, simple: str, known: set[str]):
    fields: list[tuple[str, str, bool, bool]] = []
    methods: list[tuple[str, str, bool, bool, bool]] = []
    lines = output.splitlines()
    for index, raw in enumerate(lines):
        line = raw.strip()
        if not line.endswith(";") or not raw.startswith("  "):
            continue
        descriptor = ""
        if index + 1 < len(lines):
            next_line = lines[index + 1].strip()
            if next_line.startswith("descriptor:"):
                descriptor = next_line.split(":", 1)[1].strip()
        if not descriptor:
            continue
        access = "public" if "public " in line else "protected" if "protected " in line else None
        if access is None:
            continue
        static = " static " in f" {line} "
        abstract = " abstract " in f" {line} "
        if "(" in line and not line.startswith("static {}"):
            name = line.split("(", 1)[0].rsplit(" ", 1)[-1]
            if name == simple or name.rsplit(".", 1)[-1] == simple:
                methods.append(("__init__", descriptor, static, abstract, access == "public"))
            else:
                methods.append((name, descriptor, static, abstract, access == "public"))
        else:
            name = line.rsplit(" ", 1)[-1].rstrip(";")
            fields.append((name, descriptor, static, "final" in line))
    return fields, methods


def generate_class(paper: Path, javap: str, name: str, known: set[str]) -> str | None:
    if "$" in name:
        return None
    proc = subprocess.run(
        [javap, "-p", "-s", "-classpath", str(paper), name.replace("/", ".")],
        text=True,
        capture_output=True,
        encoding="utf-8",
        errors="replace",
    )
    if proc.returncode != 0:
        return None
    output = proc.stdout
    class_line = next((line.strip() for line in output.splitlines() if " class " in line or " interface " in line), None)
    if not class_line:
        return None
    interface = " interface " in f" {class_line} "
    simple = name.rsplit("/", 1)[-1].split("$")[-1]
    package = name.rsplit("/", 1)[0].replace("/", ".")
    kind = "interface" if interface else "class"
    modifiers = "public "
    if not interface:
        modifiers += "final " if " final " in f" {class_line} " else ""
        if " abstract " in f" {class_line} ":
            modifiers += "abstract "

    fields, methods = parse_members(output, simple, known)
    body: list[str] = []
    body.append(f"package {package};")
    body.append("")
    body.append(f"/** Автоматически сгенерированный NMS-стаб для {name}; не редактировать вручную. */")
    body.append(f"{modifiers}{kind} {simple} {{")
    for field_name, descriptor, static, final in fields:
        type_name, _ = descriptor_type(descriptor, 0, known)
        mods = "public static" if static else "public"
        if static and final:
            mods += " final"
        body.append(f"    {mods} {type_name} {field_name}"
                    + (f" = {default_value(type_name)};" if static and final else ";"))
    body.append("")
    for method_name, descriptor, static, abstract, public in methods:
        if not public:
            continue
        pos = 1
        params: list[str] = []
        if descriptor.startswith("("):
            end = descriptor.index(")")
            pos = 1
            while pos < end:
                type_name, pos = descriptor_type(descriptor, pos, known)
                params.append(f"{type_name} p{len(params)}")
            return_type, _ = descriptor_type(descriptor, end + 1, known)
        else:
            continue
        mods = "public" + (" static" if static else "")
        if method_name == "__init__":
            body.append(f"    {mods} {simple}({', '.join(params)}) {{")
            body.append("    }")
            continue
        if abstract or interface:
            body.append(f"    {mods} abstract {return_type} {method_name}({', '.join(params)});")
        else:
            body.append(f"    {mods} {return_type} {method_name}({', '.join(params)}) {{")
            value = default_value(return_type)
            if value:
                body.append(f"        return {value};")
            body.append("    }")
    body.append("}")
    return "\n".join(body) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--paper", required=True, type=Path)
    parser.add_argument("--plugin", required=True, type=Path)
    parser.add_argument("--sources", required=True, type=Path)
    parser.add_argument("--jar", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--entry", default="craftengine/bukkit/nms/v26_3")
    parser.add_argument("--class", dest="classes", action="append", default=[])
    args = parser.parse_args()

    known = existing_sources(args.sources)
    with zipfile.ZipFile(args.jar) as jar:
        known |= {entry[:-6] for entry in jar.namelist() if entry.endswith(".class")}
    with zipfile.ZipFile(args.paper) as paper:
        paper_classes = {entry[:-6] for entry in paper.namelist() if entry.endswith(".class")}

    # The scanner currently walks all entries. For the generator we only need
    # the selected version's classes; scan them directly here.
    refs: set[str] = set()
    with zipfile.ZipFile(args.plugin) as jar:
        for entry in jar.namelist():
            if entry.endswith(".class") and args.entry in entry:
                from scan_plugin_nms import class_refs
                refs |= class_refs(jar.read(entry))

    candidates = sorted(set(args.classes) if args.classes else (
        ref for ref in refs if ref not in known and ref in paper_classes
    ))
    javap = find_javap()
    generated = 0
    for name in candidates:
        known.add(name)
        source = generate_class(args.paper, javap, name, known)
        if source is None:
            print(f"SKIP {name}")
            continue
        path = args.out / (name + ".java")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source, encoding="utf-8")
        print(f"GEN  {name} -> {path}")
        generated += 1
    print(f"generated {generated} class(es)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
