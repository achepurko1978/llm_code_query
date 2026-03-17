#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


SERVER_NAME = "clang-cpp-mcp"
SERVER_VERSION = "0.1.0"


def read_mcp_message() -> dict[str, Any] | None:
    headers: dict[str, str] = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.decode("utf-8", errors="replace")
        if line in ("\r\n", "\n"):
            break
        if ":" in line:
            k, v = line.split(":", 1)
            headers[k.strip().lower()] = v.strip()

    content_length = int(headers.get("content-length", "0"))
    if content_length <= 0:
        return None

    body = sys.stdin.buffer.read(content_length)
    if not body:
        return None
    return json.loads(body.decode("utf-8"))


def write_mcp_message(payload: dict[str, Any]) -> None:
    raw = json.dumps(payload, ensure_ascii=True).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(raw)}\r\n\r\n".encode("ascii"))
    sys.stdout.buffer.write(raw)
    sys.stdout.buffer.flush()


def make_response(msg_id: Any, result: dict[str, Any]) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": msg_id, "result": result}


def make_error(msg_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}}


def load_tools_schema(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as f:
        data = json.load(f)
    return list(data.get("tools", []))


def map_tool_name_to_cmd(name: str) -> str | None:
    m = {
        "cpp_resolve_symbol": "cpp_resolve_symbol",
        "cpp_semantic_query": "cpp_semantic_query",
        "cpp_describe_symbol": "cpp_describe_symbol",
    }
    return m.get(name)


def run_backend(clang_script: Path, build_dir: str, src_file: str, cmd: str, args_obj: dict[str, Any], timeout_sec: int) -> dict[str, Any]:
    command = [
        sys.executable,
        str(clang_script),
        "--build-dir",
        build_dir,
        "--file",
        src_file,
        cmd,
        "--request-json",
        json.dumps(args_obj, ensure_ascii=True),
    ]
    try:
        proc = subprocess.run(command, capture_output=True, text=True, timeout=timeout_sec)
    except subprocess.TimeoutExpired:
        return {
            "status": "error",
            "warnings": [
                {
                    "code": "BACKEND_TIMEOUT",
                    "message": f"backend timed out for {src_file} after {timeout_sec}s",
                }
            ],
        }
    if proc.returncode != 0:
        stderr = (proc.stderr or "").strip()
        stdout = (proc.stdout or "").strip()
        msg = stderr or stdout or f"backend failed with exit code {proc.returncode}"
        return {"status": "error", "warnings": [{"code": "BACKEND_ERROR", "message": msg}]}

    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        return {"status": "error", "warnings": [{"code": "BACKEND_BAD_JSON", "message": str(e)}]}


def norm_path(path_str: str, base: Path) -> Path:
    p = Path(path_str).expanduser()
    if not p.is_absolute():
        p = (base / p)
    return p.resolve()


def find_compile_db(workspace_root: Path, build_dir: str | None) -> Path:
    if build_dir:
        b = norm_path(build_dir, workspace_root)
        if b.is_file() and b.name == "compile_commands.json":
            return b
        candidate = b / "compile_commands.json"
        if candidate.is_file():
            return candidate
        raise FileNotFoundError(f"compile_commands.json not found under build-dir: {b}")

    common = [
        workspace_root / "build" / "compile_commands.json",
        workspace_root / "out" / "build" / "compile_commands.json",
        workspace_root / "cmake-build-debug" / "compile_commands.json",
        workspace_root / "cmake-build-release" / "compile_commands.json",
    ]
    for c in common:
        if c.is_file():
            return c

    matches = []
    for p in workspace_root.rglob("compile_commands.json"):
        text = str(p)
        if "/.git/" in text or "/node_modules/" in text:
            continue
        matches.append(p)

    if not matches:
        raise FileNotFoundError("could not discover compile_commands.json; run CMake configure first")

    matches.sort(key=lambda p: (len(p.parts), str(p)))
    return matches[0]


def source_files_from_compile_db(compile_db: Path, workspace_root: Path, build_dir: Path) -> list[str]:
    with compile_db.open("r", encoding="utf-8") as f:
        data = json.load(f)

    out = []
    seen = set()
    for entry in data if isinstance(data, list) else []:
        if not isinstance(entry, dict):
            continue
        raw_file = entry.get("file")
        raw_dir = entry.get("directory")
        if not isinstance(raw_file, str):
            continue

        fp = Path(raw_file)
        if not fp.is_absolute() and isinstance(raw_dir, str):
            fp = Path(raw_dir) / fp
        fp = fp.resolve()
        if not fp.is_file():
            continue

        # Keep only project C/C++ files under workspace and skip generated build-tree files.
        if workspace_root not in fp.parents and fp != workspace_root:
            continue
        if build_dir in fp.parents:
            continue
        if fp.suffix.lower() not in {".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp", ".hxx"}:
            continue

        key = str(fp)
        if key in seen:
            continue
        seen.add(key)
        out.append(key)

    return out


def parse_int(value: Any, default: int, low: int, high: int) -> int:
    try:
        x = int(value)
    except Exception:
        x = default
    return max(low, min(high, x))


def parse_cursor(value: Any) -> int:
    try:
        x = int(value)
        return x if x >= 0 else 0
    except Exception:
        return 0


def dedupe_items(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out = []
    seen = set()
    for item in items:
        sid = item.get("symbol_id")
        key = f"sid:{sid}" if isinstance(sid, str) and sid else f"json:{json.dumps(item, sort_keys=True)}"
        if key in seen:
            continue
        seen.add(key)
        out.append(item)
    return out


def is_no_match_describe(payload: dict[str, Any]) -> bool:
    item = payload.get("item")
    if not isinstance(item, dict):
        return True
    if item.get("name"):
        return False
    for w in payload.get("warnings", []):
        if isinstance(w, dict) and w.get("code") == "NO_MATCH":
            return True
    return True


def target_files_for_tool(cmd: str, call_args: dict[str, Any], all_files: list[str], workspace_root: Path) -> list[str]:
    if cmd == "cpp_resolve_symbol":
        f = call_args.get("file")
        if isinstance(f, str) and f:
            p = norm_path(f, workspace_root)
            return [str(p)] if p.is_file() else []
        return all_files

    if cmd == "cpp_semantic_query":
        scope = call_args.get("scope")
        if isinstance(scope, dict):
            f = scope.get("file")
            if isinstance(f, str) and f:
                p = norm_path(f, workspace_root)
                return [str(p)] if p.is_file() else []
        return all_files

    return all_files


def route_tool_call(clang_script: Path, build_dir: str, workspace_root: Path, files: list[str], cmd: str, call_args: dict[str, Any], timeout_sec: int) -> dict[str, Any]:
    if not files:
        return {"status": "error", "warnings": [{"code": "NO_SOURCE_FILES", "message": "no source files found in compile_commands.json"}]}

    targets = target_files_for_tool(cmd, call_args, files, workspace_root)
    if not targets:
        return {"status": "error", "warnings": [{"code": "NO_TARGET_FILES", "message": "no matching target files for request"}]}

    if cmd == "cpp_resolve_symbol":
        requested_limit = parse_int(call_args.get("limit", 20), 20, 1, 100)
        req = dict(call_args)
        req["limit"] = 1000

        all_items: list[dict[str, Any]] = []
        all_warnings: list[dict[str, Any]] = []
        had_error = False

        for src in targets:
            payload = run_backend(clang_script, build_dir, src, cmd, req, timeout_sec)
            if payload.get("status") == "error":
                had_error = True
                all_warnings.extend(payload.get("warnings", []))
                continue
            all_items.extend(payload.get("items", []))
            all_warnings.extend(payload.get("warnings", []))

        items = dedupe_items(all_items)
        total = len(items)
        out_items = items[:requested_limit]
        truncated = requested_limit < total
        return {
            "status": "error" if had_error and not out_items else "ok",
            "result_kind": "resolve_symbol",
            "ambiguous": total > 1,
            "items": out_items,
            "warnings": all_warnings,
            "page": {
                "next_cursor": str(requested_limit) if truncated else None,
                "truncated": truncated,
                "total_matches": total,
            },
        }

    if cmd == "cpp_semantic_query":
        action = call_args.get("action")
        if action not in {"find", "list", "count", "exists"}:
            return {"status": "error", "result_kind": "list", "warnings": [{"code": "INVALID_REQUEST", "message": "action must be one of find|list|count|exists"}], "items": [], "page": {"next_cursor": None, "truncated": False, "total_matches": 0}}

        if action in {"find", "list"}:
            requested_limit = parse_int(call_args.get("limit", 100), 100, 1, 1000)
            requested_cursor = parse_cursor(call_args.get("cursor"))

            req = dict(call_args)
            req["limit"] = 1000
            req.pop("cursor", None)

            all_items: list[dict[str, Any]] = []
            all_warnings: list[dict[str, Any]] = []
            had_error = False

            for src in targets:
                payload = run_backend(clang_script, build_dir, src, cmd, req, timeout_sec)
                if payload.get("status") == "error":
                    had_error = True
                    all_warnings.extend(payload.get("warnings", []))
                    continue
                all_items.extend(payload.get("items", []))
                all_warnings.extend(payload.get("warnings", []))

            items = dedupe_items(all_items)
            total = len(items)
            sliced = items[requested_cursor: requested_cursor + requested_limit]
            next_cursor = requested_cursor + len(sliced)
            truncated = next_cursor < total
            return {
                "status": "error" if had_error and not sliced else "ok",
                "result_kind": action,
                "items": sliced,
                "warnings": all_warnings,
                "page": {
                    "next_cursor": str(next_cursor) if truncated else None,
                    "truncated": truncated,
                    "total_matches": total,
                },
            }

        if action == "count":
            total = 0
            all_warnings: list[dict[str, Any]] = []
            had_error = False
            for src in targets:
                payload = run_backend(clang_script, build_dir, src, cmd, call_args, timeout_sec)
                if payload.get("status") == "error":
                    had_error = True
                    all_warnings.extend(payload.get("warnings", []))
                    continue
                total += int(payload.get("count", 0))
                all_warnings.extend(payload.get("warnings", []))
            return {"status": "error" if had_error and total == 0 else "ok", "result_kind": "count", "count": total, "warnings": all_warnings}

        exists = False
        all_warnings: list[dict[str, Any]] = []
        had_error = False
        for src in targets:
            payload = run_backend(clang_script, build_dir, src, cmd, call_args, timeout_sec)
            if payload.get("status") == "error":
                had_error = True
                all_warnings.extend(payload.get("warnings", []))
                continue
            exists = exists or bool(payload.get("exists", False))
            all_warnings.extend(payload.get("warnings", []))
        return {"status": "error" if had_error and not exists else "ok", "result_kind": "exists", "exists": exists, "warnings": all_warnings}

    # cpp_describe_symbol
    last_no_match = {
        "status": "ok",
        "result_kind": "describe_symbol",
        "item": {"symbol_id": str(call_args.get("symbol_id", "")), "entity": "file", "name": ""},
        "warnings": [{"code": "NO_MATCH", "message": f"symbol not found: {call_args.get('symbol_id', '')}"}],
    }

    for src in targets:
        payload = run_backend(clang_script, build_dir, src, cmd, call_args, timeout_sec)
        if payload.get("status") == "error":
            continue
        if not is_no_match_describe(payload):
            return payload
        last_no_match = payload

    return last_no_match


def tool_result(payload: dict[str, Any], is_error: bool = False) -> dict[str, Any]:
    txt = json.dumps(payload, indent=2, ensure_ascii=True)
    return {
        "content": [{"type": "text", "text": txt}],
        "structuredContent": payload,
        "isError": is_error,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace-root", default=".")
    parser.add_argument("--build-dir")
    parser.add_argument("--backend-timeout", type=int, default=12)
    parser.add_argument("--tools-json", default="tools.json")
    parser.add_argument("--clang-script", default="clang_mcp.py")
    args = parser.parse_args()

    base_dir = Path.cwd()
    workspace_root = norm_path(args.workspace_root, base_dir)
    tools_path = norm_path(args.tools_json, workspace_root)
    clang_script = norm_path(args.clang_script, workspace_root)

    tools = load_tools_schema(tools_path)

    compile_db = find_compile_db(workspace_root, args.build_dir)
    build_dir = str(compile_db.parent)
    source_files = source_files_from_compile_db(compile_db, workspace_root, compile_db.parent)

    while True:
        req = read_mcp_message()
        if req is None:
            return 0

        method = req.get("method")
        msg_id = req.get("id")

        if method == "initialize":
            result = {
                "protocolVersion": req.get("params", {}).get("protocolVersion", "2024-11-05"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            }
            if msg_id is not None:
                write_mcp_message(make_response(msg_id, result))
            continue

        if method == "notifications/initialized":
            continue

        if method == "tools/list":
            out_tools = []
            for t in tools:
                out_tools.append(
                    {
                        "name": t.get("name"),
                        "description": t.get("description", ""),
                        "inputSchema": t.get("inputSchema", {"type": "object"}),
                    }
                )
            if msg_id is not None:
                write_mcp_message(make_response(msg_id, {"tools": out_tools}))
            continue

        if method == "tools/call":
            params = req.get("params", {})
            name = params.get("name")
            call_args = params.get("arguments", {})
            cmd = map_tool_name_to_cmd(str(name))
            if not cmd:
                if msg_id is not None:
                    write_mcp_message(make_response(msg_id, tool_result({"status": "error", "warnings": [{"code": "UNKNOWN_TOOL", "message": f"unknown tool: {name}"}]}, is_error=True)))
                continue

            if not isinstance(call_args, dict):
                call_args = {}

            payload = route_tool_call(clang_script, build_dir, workspace_root, source_files, cmd, call_args, args.backend_timeout)
            is_error = payload.get("status") == "error"
            if msg_id is not None:
                write_mcp_message(make_response(msg_id, tool_result(payload, is_error=is_error)))
            continue

        if msg_id is not None:
            write_mcp_message(make_error(msg_id, -32601, f"Method not found: {method}"))


if __name__ == "__main__":
    raise SystemExit(main())
