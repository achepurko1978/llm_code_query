#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


SERVER_NAME = "clang-cpp-mcp"
SERVER_VERSION = "0.1.0"

# Accumulated leftover bytes from previous reads.
_stdin_buf = bytearray()


def _log(msg: str) -> None:
    os.write(2, f"[clang-cpp-mcp] {msg}\n".encode("utf-8"))


def _fill_buf(min_bytes: int = 1) -> bool:
    """Read from stdin until _stdin_buf has at least min_bytes, or EOF."""
    global _stdin_buf
    while len(_stdin_buf) < min_bytes:
        chunk = os.read(0, 65536)
        if not chunk:
            return False
        _stdin_buf.extend(chunk)
    return True


def _consume(n: int) -> bytes:
    global _stdin_buf
    data = bytes(_stdin_buf[:n])
    del _stdin_buf[:n]
    return data


def _peek_byte() -> int | None:
    if not _fill_buf(1):
        return None
    return _stdin_buf[0]


def _read_content_length_message() -> dict[str, Any] | None:
    """Read a Content-Length framed JSON-RPC message."""
    global _stdin_buf
    while True:
        if not _fill_buf(1):
            return None
        idx = _stdin_buf.find(b"\r\n\r\n")
        if idx >= 0:
            break
        # Need more data for full header
        old_len = len(_stdin_buf)
        if not _fill_buf(old_len + 1):
            return None

    header_block = bytes(_stdin_buf[:idx]).decode("utf-8", errors="replace")
    del _stdin_buf[:idx + 4]  # consume header + \r\n\r\n

    content_length = 0
    for line in header_block.split("\r\n"):
        if ":" in line:
            k, v = line.split(":", 1)
            if k.strip().lower() == "content-length":
                content_length = int(v.strip())

    if content_length <= 0:
        return None

    if not _fill_buf(content_length):
        return None
    body = _consume(content_length)
    return json.loads(body.decode("utf-8"))


def _read_newline_json_message() -> dict[str, Any] | None:
    """Read a newline-delimited JSON message."""
    global _stdin_buf
    while True:
        idx = _stdin_buf.find(b"\n")
        if idx >= 0:
            line = _consume(idx + 1)
            stripped = line.strip()
            if stripped:
                return json.loads(stripped.decode("utf-8"))
            continue  # blank line, skip
        old_len = len(_stdin_buf)
        if not _fill_buf(old_len + 1):
            # EOF — try to parse whatever remains
            if _stdin_buf:
                data = bytes(_stdin_buf)
                _stdin_buf.clear()
                stripped = data.strip()
                if stripped:
                    return json.loads(stripped.decode("utf-8"))
            return None


def read_mcp_message() -> dict[str, Any] | None:
    """Auto-detect framing: Content-Length headers or newline-delimited JSON."""
    b = _peek_byte()
    if b is None:
        _log("stdin EOF")
        return None
    if chr(b) == "{":
        return _read_newline_json_message()
    elif chr(b) == "C":
        return _read_content_length_message()
    else:
        # Skip whitespace / blank lines
        _consume(1)
        return read_mcp_message()


def write_mcp_message(payload: dict[str, Any]) -> None:
    raw = json.dumps(payload, ensure_ascii=True).encode("utf-8")
    os.write(1, raw + b"\n")


def _write_all(data: bytes) -> None:
    mv = memoryview(data)
    while mv:
        written = os.write(1, mv)
        mv = mv[written:]


def make_response(msg_id: Any, result: dict[str, Any]) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": msg_id, "result": result}


def make_error(msg_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}}


def _resolve_refs(obj: Any, defs: dict[str, Any], depth: int = 0) -> Any:
    if depth > 20:
        return obj
    if isinstance(obj, dict):
        ref = obj.get("$ref")
        if ref and isinstance(ref, str):
            prefix = "cpp-mcp-v1.schema.json#/$defs/"
            if ref.startswith(prefix):
                def_name = ref[len(prefix):]
            elif ref.startswith("#/$defs/"):
                def_name = ref[len("#/$defs/"):]
            else:
                def_name = None
            if def_name and def_name in defs:
                return _resolve_refs(defs[def_name], defs, depth + 1)
        out = {}
        for k, v in obj.items():
            if k == "additionalProperties":
                continue
            out[k] = _resolve_refs(v, defs, depth + 1)
        return out
    if isinstance(obj, list):
        return [_resolve_refs(v, defs, depth + 1) for v in obj]
    return obj


def load_tools_schema(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as f:
        data = json.load(f)
    tools = list(data.get("tools", []))

    schema_path = path.parent / "cpp-mcp-v1.schema.json"
    defs: dict[str, Any] = {}
    if schema_path.is_file():
        with schema_path.open("r", encoding="utf-8") as f:
            schema = json.load(f)
        defs = schema.get("$defs", {})

    return [_resolve_refs(t, defs) for t in tools]


def map_tool_name_to_cmd(name: str) -> str | None:
    m = {
        "cpp_resolve_symbol": "cpp_resolve_symbol",
        "cpp_semantic_query": "cpp_semantic_query",
        "cpp_describe_symbol": "cpp_describe_symbol",
    }
    return m.get(name)


def run_backend(clang_script: Path, build_dir: str, src_file: str, cmd: str, args_obj: dict[str, Any], timeout_sec: int, workspace_root: str | None = None) -> dict[str, Any]:
    if ".py" == clang_script.suffix.lower():
        command = [
            sys.executable,
            str(clang_script),
            "--build-dir",
            build_dir,
            "--file",
            src_file,
        ]
    else:
        command = [
            str(clang_script),
            "--build-dir",
            build_dir,
            "--file",
            src_file,
        ]
    if workspace_root:
        command += ["--workspace-root", workspace_root]
    command += [
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
    return matches[0].resolve()


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
            payload = run_backend(clang_script, build_dir, src, cmd, req, timeout_sec, str(workspace_root))
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
                payload = run_backend(clang_script, build_dir, src, cmd, req, timeout_sec, str(workspace_root))
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
                payload = run_backend(clang_script, build_dir, src, cmd, call_args, timeout_sec, str(workspace_root))
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
            payload = run_backend(clang_script, build_dir, src, cmd, call_args, timeout_sec, str(workspace_root))
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
        payload = run_backend(clang_script, build_dir, src, cmd, call_args, timeout_sec, str(workspace_root))
        if payload.get("status") == "error":
            continue
        if not is_no_match_describe(payload):
            return payload
        last_no_match = payload

    return last_no_match


def resolve_runtime_context(workspace_root: Path, build_dir_arg: str | None) -> tuple[str, list[str]]:
    compile_db = find_compile_db(workspace_root, build_dir_arg)
    build_dir = str(compile_db.parent)
    source_files = source_files_from_compile_db(compile_db, workspace_root, compile_db.parent)
    return build_dir, source_files


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
    _log(f"loaded {len(tools)} tools from {tools_path}")

    build_dir: str | None = None
    source_files: list[str] | None = None

    while True:
        req = read_mcp_message()
        if req is None:
            _log("read_mcp_message returned None, exiting")
            return 0

        method = req.get("method")
        msg_id = req.get("id")
        _log(f"received method={method} id={msg_id}")

        if method == "initialize":
            result = {
                "protocolVersion": req.get("params", {}).get("protocolVersion", "2024-11-05"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            }
            if msg_id is not None:
                write_mcp_message(make_response(msg_id, result))
                _log("sent initialize response")
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

            if build_dir is None or source_files is None:
                try:
                    build_dir, source_files = resolve_runtime_context(workspace_root, args.build_dir)
                except Exception as e:
                    payload = {
                        "status": "error",
                        "warnings": [{"code": "RUNTIME_CONTEXT_ERROR", "message": str(e)}],
                    }
                    if msg_id is not None:
                        write_mcp_message(make_response(msg_id, tool_result(payload, is_error=True)))
                    continue

            payload = route_tool_call(clang_script, build_dir, workspace_root, source_files, cmd, call_args, args.backend_timeout)
            is_error = payload.get("status") == "error"
            if msg_id is not None:
                write_mcp_message(make_response(msg_id, tool_result(payload, is_error=is_error)))
            continue

        if msg_id is not None:
            write_mcp_message(make_error(msg_id, -32601, f"Method not found: {method}"))


if __name__ == "__main__":
    raise SystemExit(main())
