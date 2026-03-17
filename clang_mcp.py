#!/usr/bin/env python3
from __future__ import annotations

import argparse, json, sys
from pathlib import Path

try:
    from clang import cindex
except ModuleNotFoundError:
    cindex = None


def die(msg: str) -> None:
    raise SystemExit(msg)


def norm(p: str) -> str:
    return str(Path(p).expanduser().resolve())


def compile_args(build_dir: str, src: str) -> list[str]:
    if cindex is None:
        die("missing dependency: install Python clang bindings with 'python3 -m pip install clang'")
    db = cindex.CompilationDatabase.fromDirectory(build_dir)
    cmds = db.getCompileCommands(src)
    cmd = next(iter(cmds or []), None)
    if not cmd:
        die(f"no compile command for {src}")
    args = list(cmd.arguments)[1:]
    out, skip = [], False
    paired = {"-o", "-MF", "-MT", "-MQ", "-MJ", "-Xclang", "-include", "-imacros", "-isysroot", "-target", "--target", "-ivfsoverlay"}
    single = {"-c", "-M", "-MM", "-MD", "-MMD", "-MP", "-Winvalid-pch"}
    srcs = {norm(src), norm(str(Path(cmd.directory) / cmd.filename))}
    for a in args:
        if skip:
            skip = False
            continue
        if a in single:
            continue
        if a in paired:
            skip = True
            continue
        if a.startswith(("-o", "-MF", "-MT", "-MQ", "-MJ")) and a not in paired:
            continue
        if not a.startswith("-") and norm(str(Path(cmd.directory) / a)) in srcs:
            continue
        out.append(a)
    return out


def parse(build_dir: str, src: str):
    if cindex is None:
        die("missing dependency: install Python clang bindings with 'python3 -m pip install clang'")
    return cindex.Index.create().parse(src, args=compile_args(build_dir, src))


def same_file(cur, src: str) -> bool:
    f = cur.location.file
    return bool(f and norm(f.name) == norm(src))


def functions(tu, src: str):
    if cindex is None:
        die("missing dependency: install Python clang bindings with 'python3 -m pip install clang'")
    for c in tu.cursor.get_children():
        if c.kind == cindex.CursorKind.FUNCTION_DECL and c.is_definition() and same_file(c, src):
            yield c


def t(typ) -> str:
    return (typ.spelling or "").strip()


def sig(c) -> str:
    ps = []
    for p in c.get_arguments() or []:
        s = t(p.type)
        if p.spelling:
            s += f" {p.spelling}"
        ps.append(s)
    return f"{t(c.result_type)} {c.spelling}({', '.join(ps)})"


def item(c) -> dict:
    loc = c.location
    return {
        "symbol_id": c.get_usr() or f"loc:{loc.file.name}:{loc.line}:{loc.column}",
        "entity": "function",
        "name": c.spelling,
        "qualified_name": c.spelling,
        "signature": sig(c),
        "return_type": t(c.result_type),
        "location": {"file": str(loc.file.name), "line": loc.line, "column": loc.column},
    }


def list_functions(build_dir: str, src: str) -> dict:
    xs = [item(c) for c in functions(parse(build_dir, src), src)]
    return {"status": "ok", "result_kind": "list", "items": xs, "warnings": [], "page": {"next_cursor": None, "truncated": False, "total_matches": len(xs)}}


def describe_function(build_dir: str, src: str, name: str) -> dict:
    ms = [c for c in functions(parse(build_dir, src), src) if c.spelling == name]
    if not ms:
        return {"status": "ok", "result_kind": "describe_symbol", "item": None, "warnings": [{"code": "NO_MATCH", "message": f"no function named {name}"}]}
    if len(ms) > 1:
        return {"status": "ok", "result_kind": "describe_symbol", "item": None, "warnings": [{"code": "AMBIGUOUS_SYMBOL", "message": f"multiple functions named {name}"}], "candidates": [item(c) for c in ms]}
    c = ms[0]
    out = item(c)
    out["parameters"] = [{"name": p.spelling or "", "type": t(p.type), "position": i} for i, p in enumerate(c.get_arguments() or [])]
    return {"status": "ok", "result_kind": "describe_symbol", "item": out, "warnings": []}


def doctor(build_dir: str | None, src: str | None) -> dict:
    checks = []

    if cindex is None:
        checks.append({"name": "python_clang_module", "ok": False, "message": "missing Python clang bindings"})
    else:
        checks.append({"name": "python_clang_module", "ok": True, "message": "clang.cindex import succeeded"})
        try:
            cindex.Index.create()
            checks.append({"name": "libclang_runtime", "ok": True, "message": "libclang runtime is usable"})
        except Exception as e:
            checks.append({"name": "libclang_runtime", "ok": False, "message": f"libclang unavailable: {e}"})

    if build_dir:
        p = Path(build_dir)
        db_path = p / "compile_commands.json"
        checks.append({"name": "build_dir_exists", "ok": p.is_dir(), "message": str(p)})
        checks.append({"name": "compile_commands_json", "ok": db_path.is_file(), "message": str(db_path)})

    if src:
        p = Path(src)
        checks.append({"name": "source_file_exists", "ok": p.is_file(), "message": str(p)})

    ok = all(c["ok"] for c in checks)
    warnings = [] if ok else [{"code": "CHECK_FAILED", "message": "one or more doctor checks failed"}]
    return {"status": "ok", "result_kind": "doctor", "ok": ok, "checks": checks, "warnings": warnings}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--build-dir")
    ap.add_argument("--file")
    sp = ap.add_subparsers(dest="cmd", required=True)
    sp.add_parser("list-functions")
    sp.add_parser("doctor")
    p = sp.add_parser("describe-function")
    p.add_argument("--name", required=True)

    if len(sys.argv) == 1:
        ap.print_help(sys.stdout)
        return 0

    a = ap.parse_args()
    src = norm(a.file) if a.file else None
    build = norm(a.build_dir) if a.build_dir else None

    if a.cmd == "doctor":
        out = doctor(build, src)
    else:
        if not build or not src:
            die("--build-dir and --file are required for list-functions and describe-function")
        out = list_functions(build, src) if a.cmd == "list-functions" else describe_function(build, src, a.name)
    json.dump(out, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
