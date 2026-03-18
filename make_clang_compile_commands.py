#!/usr/bin/env python3
"""
Generate a clang-compatible compile_commands.json for workspace source files only.

Run after every CMake configure:
    python3 llm_code_query/make_clang_compile_commands.py

Output: build/clang/compile_commands.json
"""
from __future__ import annotations

import json
import shlex
import sys
from pathlib import Path

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
BUILD_DIR = WORKSPACE_ROOT / "build" / "obj" / "x86_64" / "debug"
OUTPUT_DIR = WORKSPACE_ROOT / "build" / "clang"
OUTPUT_FILE = OUTPUT_DIR / "compile_commands.json"

CLANG_CXX = "/opt/clang/bin/clang++"

# Flags that are GCC-specific and unknown / harmful to clang.
# Each is matched as a prefix of a token (covers both -flag and -flag=value forms).
STRIP_PREFIXES: tuple[str, ...] = (
    "-fmodules-ts",
    "-fmodule-mapper",
    "-fdeps-format",
    "-MD",
    "-Wformat-truncation",
)


def _is_stripped(token: str) -> bool:
    return any(token.startswith(p) for p in STRIP_PREFIXES)


def _transform_command(raw_command: str) -> str:
    tokens = shlex.split(raw_command)
    out: list[str] = []
    skip_next = False
    for i, tok in enumerate(tokens):
        if skip_next:
            skip_next = False
            continue
        if i == 0:
            # Replace compiler with clang++
            out.append(CLANG_CXX)
            continue
        if _is_stripped(tok):
            # Some GCC flags take a separate value token (no '=' form).
            # Check if next token looks like a value (no leading '-').
            if "=" not in tok and i + 1 < len(tokens) and not tokens[i + 1].startswith("-"):
                skip_next = True
            continue
        if tok == "-std=gnu++20":
            out.append("-std=c++20")
            continue
        out.append(tok)
    return shlex.join(out)


def _is_workspace_file(path_str: str) -> bool:
    p = Path(path_str).resolve()
    try:
        rel = p.relative_to(WORKSPACE_ROOT)
    except ValueError:
        return False
    # Exclude generated build-tree files
    parts = rel.parts
    return len(parts) > 0 and parts[0] != "build"


def main() -> None:
    input_file = BUILD_DIR / "compile_commands.json"
    if not input_file.is_file():
        sys.exit(f"error: compile_commands.json not found at {input_file}\nRun CMake configure first.")

    with input_file.open(encoding="utf-8") as f:
        db: list[dict] = json.load(f)

    result = []
    skipped = 0
    for entry in db:
        src = entry.get("file", "")
        if not _is_workspace_file(src):
            skipped += 1
            continue
        new_entry = dict(entry)
        new_entry["command"] = _transform_command(entry["command"])
        result.append(new_entry)

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with OUTPUT_FILE.open("w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)

    print(f"Written {len(result)} entries ({skipped} skipped) -> {OUTPUT_FILE}")


if __name__ == "__main__":
    main()
