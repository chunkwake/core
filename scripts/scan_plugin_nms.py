#!/usr/bin/env python3
"""Scan plugin jars for net.minecraft.* class references missing from the API layer.

This is the first half of the automatic NMS-stub generator: it tells us exactly
which vanilla classes a plugin set touches, instead of waiting for the next
NoClassDefFoundError in the server log.
"""

from __future__ import annotations

import argparse
import struct
import sys
import zipfile
from pathlib import Path


def read_u1(data: bytes, pos: int) -> tuple[int, int]:
    return data[pos], pos + 1


def read_u2(data: bytes, pos: int) -> tuple[int, int]:
    return struct.unpack_from(">H", data, pos)[0], pos + 2


def read_u4(data: bytes, pos: int) -> tuple[int, int]:
    return struct.unpack_from(">I", data, pos)[0], pos + 4


def class_refs(data: bytes) -> set[str]:
    """Minimal constant-pool reader: all CONSTANT_Class names."""
    magic, pos = read_u4(data, 0)
    if magic != 0xCAFEBABE:
        return set()
    _minor, pos = read_u2(data, pos)
    _major, pos = read_u2(data, pos)
    count, pos = read_u2(data, pos)
    utf8: dict[int, str] = {}
    class_names: dict[int, int] = {}
    index = 1
    while index < count:
        tag, pos = read_u1(data, pos)
        if tag == 1:  # Utf8
            length, pos = read_u2(data, pos)
            utf8[index] = data[pos:pos + length].decode("utf-8", "replace")
            pos += length
        elif tag in (7, 8, 16, 19, 20):  # Class, String, MethodType, Module, Package
            value, pos = read_u2(data, pos)
            if tag == 7:
                class_names[index] = value
        elif tag in (15,):  # MethodHandle: u1 ref_kind + u2 ref_index
            _kind, pos = read_u1(data, pos)
            _ref, pos = read_u2(data, pos)
        elif tag in (3, 4, 9, 10, 11, 12, 17, 18):  # 4-byte or two-u2
            pos += 4
        elif tag in (5, 6):  # long/double take two slots
            pos += 8
            index += 1
        else:
            return set()
        index += 1
    return {
        utf8[name_index].replace(".", "/")
        for name_index in class_names.values()
        if name_index in utf8 and utf8[name_index].startswith("net/minecraft/")
    }


def scan_jar(path: Path) -> set[str]:
    refs: set[str] = set()
    with zipfile.ZipFile(path) as jar:
        for entry in jar.namelist():
            if not entry.endswith(".class"):
                continue
            try:
                refs |= class_refs(jar.read(entry))
            except (KeyError, struct.error, UnicodeDecodeError):
                pass
    return refs


def existing_sources(root: Path) -> set[str]:
    classes: set[str] = set()
    for path in root.rglob("*.java"):
        rel = path.relative_to(root).with_suffix("")
        classes.add("/".join(rel.parts))
    return classes


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--paper", required=True, type=Path)
    parser.add_argument("--plugins", required=True, type=Path)
    parser.add_argument("--sources", required=True, type=Path)
    parser.add_argument("--jar", type=Path)
    args = parser.parse_args()

    existing = existing_sources(args.sources)
    if args.jar and args.jar.is_file():
        with zipfile.ZipFile(args.jar) as jar:
            existing |= {
                entry[:-6]
                for entry in jar.namelist()
                if entry.endswith(".class")
            }

    refs: set[str] = set()
    plugin_jars = sorted(args.plugins.rglob("*.jar"))
    for path in plugin_jars:
        refs |= scan_jar(path)

    with zipfile.ZipFile(args.paper) as paper:
        paper_classes = {
            entry[:-6]
            for entry in paper.namelist()
            if entry.endswith(".class")
        }

    missing = sorted(ref for ref in refs if ref not in existing)
    top_level = [name for name in missing if "$" not in name]
    nested = [name for name in missing if "$" in name]
    print(f"plugin jars: {len(plugin_jars)}")
    print(f"net.minecraft refs: {len(refs)}")
    print(f"missing from API: {len(missing)} (top-level {len(top_level)}, nested {len(nested)})")
    for name in top_level:
        print(f"TOP  {name}" + ("" if name in paper_classes else "  [not in Paper]"))
    for name in nested[:30]:
        print(f"NEST {name}")
    if len(nested) > 30:
        print(f"... {len(nested) - 30} more nested")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
