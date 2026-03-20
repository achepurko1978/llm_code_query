#!/usr/bin/env python3
"""MCP server for C++ code analysis via clang backend.

Uses the official MCP Python SDK for protocol handling and stdio transport.
"""
from __future__ import annotations

import argparse
import fnmatch
import json
import logging
import subprocess
import sys
from pathlib import Path
from typing import Any

import anyio
from mcp import types
from mcp.server.lowlevel import Server
from mcp.server.stdio import stdio_server

SERVER_NAME = "clang-cpp-mcp"
SERVER_VERSION = "0.1.0"

logger = logging.getLogger(SERVER_NAME)

VALID_TOOLS = frozenset({"cpp_semantic_query"})
VALID_ACTIONS = frozenset({"find", "list", "count", "exists"})
CPP_EXTENSIONS = frozenset({".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp", ".hxx"})


# ---------------------------------------------------------------------------
# Schema resolution
# ---------------------------------------------------------------------------

def _resolve_refs(obj: Any, defs: dict[str, Any], depth: int = 0) -> Any:
    if depth > 20:
        return obj
    if isinstance(obj, dict):
        ref = obj.get("$ref")
        if ref and isinstance(ref, str):
            for prefix in ("cpp-mcp-v1.schema.json#/$defs/", "#/$defs/"):
                if ref.startswith(prefix):
                    def_name = ref[len(prefix):]
                    if def_name in defs:
                        return _resolve_refs(defs[def_name], defs, depth + 1)
                    break
        return {k: _resolve_refs(v, defs, depth + 1) for k, v in obj.items() if k not in ("additionalProperties", "enum")}
    if isinstance(obj, list):
        return [_resolve_refs(v, defs, depth + 1) for v in obj]
    return obj


def load_tools_schema(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as f:
        tools = list(json.load(f).get("tools", []))
    schema_path = path.parent / "cpp-mcp-v1.schema.json"
    defs: dict[str, Any] = {}
    if schema_path.is_file():
        with schema_path.open("r", encoding="utf-8") as f:
            defs = json.load(f).get("$defs", {})
    return [_resolve_refs(t, defs) for t in tools]


# ---------------------------------------------------------------------------
# Path utilities
# ---------------------------------------------------------------------------

def norm_path(path_str: str, base: Path) -> Path:
    p = Path(path_str).expanduser()
    if not p.is_absolute():
        p = base / p
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
    for subdir in ("build", "out/build", "cmake-build-debug", "cmake-build-release"):
        candidate = workspace_root / subdir / "compile_commands.json"
        if candidate.is_file():
            return candidate
    matches = [
        p for p in workspace_root.rglob("compile_commands.json")
        if "/.git/" not in str(p) and "/node_modules/" not in str(p)
    ]
    if not matches:
        raise FileNotFoundError("could not discover compile_commands.json; run CMake configure first")
    matches.sort(key=lambda p: (len(p.parts), str(p)))
    return matches[0].resolve()


def source_files_from_compile_db(compile_db: Path, workspace_root: Path, build_dir: Path) -> list[str]:
    with compile_db.open("r", encoding="utf-8") as f:
        data = json.load(f)
    if not isinstance(data, list):
        return []
    seen: set[str] = set()
    out: list[str] = []
    for entry in data:
        if not isinstance(entry, dict):
            continue
        raw_file = entry.get("file")
        if not isinstance(raw_file, str):
            continue
        fp = Path(raw_file)
        if not fp.is_absolute():
            raw_dir = entry.get("directory")
            if isinstance(raw_dir, str):
                fp = Path(raw_dir) / fp
        fp = fp.resolve()
        if (not fp.is_file()
                or fp.suffix.lower() not in CPP_EXTENSIONS
                or (workspace_root not in fp.parents and fp != workspace_root)
                or build_dir in fp.parents):
            continue
        key = str(fp)
        if key not in seen:
            seen.add(key)
            out.append(key)
    return out


def resolve_runtime_context(workspace_root: Path, build_dir_arg: str | None) -> tuple[str, list[str]]:
    compile_db = find_compile_db(workspace_root, build_dir_arg)
    build_dir = str(compile_db.parent)
    return build_dir, source_files_from_compile_db(compile_db, workspace_root, compile_db.parent)


# ---------------------------------------------------------------------------
# Backend execution
# ---------------------------------------------------------------------------

def run_backend(clang_script: Path, build_dir: str, src_file: str, cmd: str,
                args_obj: dict[str, Any], timeout_sec: int,
                workspace_root: str | None = None) -> dict[str, Any]:
    command = (
        [sys.executable, str(clang_script)] if clang_script.suffix.lower() == ".py"
        else [str(clang_script)]
    )
    command += ["--build-dir", build_dir, "--file", src_file]
    if workspace_root:
        command += ["--workspace-root", workspace_root]
    command += [cmd, "--request-json", json.dumps(args_obj, ensure_ascii=True)]
    logger.debug("backend call: %s --file %s  args=%s", cmd, src_file, json.dumps(args_obj, sort_keys=True))
    try:
        proc = subprocess.run(command, capture_output=True, text=True, timeout=timeout_sec)
    except subprocess.TimeoutExpired:
        logger.warning("backend timeout: %s --file %s after %ds", cmd, src_file, timeout_sec)
        return _backend_error("BACKEND_TIMEOUT", f"backend timed out for {src_file} after {timeout_sec}s")
    if proc.stderr:
        logger.debug("backend stderr: %s --file %s\n%s", cmd, src_file, proc.stderr.rstrip())
    if proc.returncode != 0:
        msg = (proc.stderr or "").strip() or (proc.stdout or "").strip() or f"backend failed with exit code {proc.returncode}"
        logger.warning("backend error: %s --file %s  rc=%d  %s", cmd, src_file, proc.returncode, msg[:200])
        logger.debug("backend stdout on error: %s --file %s\n%s", cmd, src_file, (proc.stdout or "").rstrip())
        return _backend_error("BACKEND_ERROR", msg)
    logger.debug("backend stdout: %s --file %s\n%s", cmd, src_file, proc.stdout.rstrip())
    try:
        result = json.loads(proc.stdout)
        logger.debug("backend ok: %s --file %s  status=%s", cmd, src_file, result.get("status"))
        return result
    except json.JSONDecodeError as e:
        logger.warning("backend bad json: %s --file %s  %s", cmd, src_file, e)
        return _backend_error("BACKEND_BAD_JSON", str(e))


def _backend_error(code: str, message: str) -> dict[str, Any]:
    return {"status": "error", "warnings": [{"code": code, "message": message}]}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

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


def _is_header_location(item: dict[str, Any]) -> bool:
    """Return True if this item's location points to a header file."""
    loc = item.get("location")
    if isinstance(loc, dict):
        f = loc.get("file", "")
        if isinstance(f, str):
            ext = Path(f).suffix.lower()
            return ext in {".h", ".hh", ".hpp", ".hxx"}
    return False


def dedupe_items(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
    seen: dict[str, int] = {}  # key -> index in out
    out: list[dict[str, Any]] = []
    for item in items:
        sid = item.get("symbol_id")
        key = f"sid:{sid}" if isinstance(sid, str) and sid else f"json:{json.dumps(item, sort_keys=True)}"
        if key not in seen:
            seen[key] = len(out)
            out.append(item)
        else:
            # Prefer source file locations over header declarations
            prev_idx = seen[key]
            if _is_header_location(out[prev_idx]) and not _is_header_location(item):
                out[prev_idx] = item
    return out


# ---------------------------------------------------------------------------
# Tool routing
# ---------------------------------------------------------------------------

def _is_glob(s: str) -> bool:
    """Return True if *s* contains glob meta-characters."""
    return any(ch in s for ch in ('*', '?', '['))


def target_files_for_tool(cmd: str, call_args: dict[str, Any], all_files: list[str],
                          workspace_root: Path) -> tuple[list[str], bool]:
    """Return (target_files, scope_was_directory_or_glob)."""
    path_str: str | None = None
    if cmd == "cpp_semantic_query":
        scope = call_args.get("scope")
        if isinstance(scope, dict):
            path_str = scope.get("path")
    if isinstance(path_str, str) and path_str:
        if _is_glob(path_str):
            # Resolve glob relative to workspace root
            pattern = str(workspace_root / path_str) if not Path(path_str).is_absolute() else path_str
            matched = [f for f in all_files if fnmatch.fnmatch(f, pattern)]
            return matched, True
        p = norm_path(path_str, workspace_root)
        if p.is_file():
            return [str(p)], False
        if p.is_dir():
            prefix = str(p) + "/"
            return [f for f in all_files if f.startswith(prefix) or f == str(p)], True
        return [], False
    return all_files, False


def _strip_dir_scope(call_args: dict[str, Any], cmd: str) -> dict[str, Any]:
    """Remove scope.path from args when targeting was already resolved to a directory/glob."""
    args = {**call_args}
    if cmd == "cpp_semantic_query":
        scope = args.get("scope")
        if isinstance(scope, dict) and "path" in scope:
            new_scope = {k: v for k, v in scope.items() if k != "path"}
            if new_scope:
                args["scope"] = new_scope
            else:
                del args["scope"]
    return args


def _aggregate_backends(clang_script: Path, build_dir: str, workspace_root: Path,
                        targets: list[str], cmd: str, call_args: dict[str, Any],
                        timeout_sec: int) -> tuple[list[dict], list[dict], bool]:
    all_items: list[dict[str, Any]] = []
    all_warnings: list[dict[str, Any]] = []
    had_error = False
    for src in targets:
        payload = run_backend(clang_script, build_dir, src, cmd, call_args, timeout_sec, str(workspace_root))
        if payload.get("status") == "error":
            had_error = True
            all_warnings.extend(payload.get("warnings", []))
            continue
        all_items.extend(payload.get("items", []))
        all_warnings.extend(payload.get("warnings", []))
    return all_items, all_warnings, had_error


def _normalize_scope_path(call_args: dict[str, Any]) -> dict[str, Any]:
    """Normalize legacy scope.file / scope.directory to scope.path."""
    scope = call_args.get("scope")
    if not isinstance(scope, dict):
        return call_args
    if "path" in scope:
        return call_args
    legacy = scope.get("file") or scope.get("directory")
    if legacy:
        new_scope = {k: v for k, v in scope.items() if k not in ("file", "directory")}
        new_scope["path"] = legacy
        return {**call_args, "scope": new_scope}
    return call_args


def _normalize_include_source_from_fields(cmd: str, call_args: dict[str, Any]) -> dict[str, Any]:
    """For semantic queries, auto-enable include_source when fields request source/extent."""
    if cmd != "cpp_semantic_query":
        return call_args

    fields = call_args.get("fields")
    if not isinstance(fields, list):
        return call_args

    wants_source = any(f in ("source", "extent") for f in fields if isinstance(f, str))
    if not wants_source:
        return call_args

    if call_args.get("include_source") is True:
        return call_args

    return {**call_args, "include_source": True}


def route_tool_call(clang_script: Path, build_dir: str, workspace_root: Path,
                    files: list[str], cmd: str, call_args: dict[str, Any],
                    timeout_sec: int) -> dict[str, Any]:
    if not files:
        return _backend_error("NO_SOURCE_FILES", "no source files found in compile_commands.json")
    call_args = _normalize_scope_path(call_args)
    call_args = _normalize_include_source_from_fields(cmd, call_args)
    targets, dir_scope = target_files_for_tool(cmd, call_args, files, workspace_root)
    if not targets:
        return _backend_error("NO_TARGET_FILES", "no matching target files for request")
    # When scope resolved to a directory, strip it from backend args since
    # each backend invocation already receives a specific --file.
    if dir_scope:
        call_args = _strip_dir_scope(call_args, cmd)

    # cpp_semantic_query
    action = call_args.get("action")
    if action not in VALID_ACTIONS:
        return {"status": "error", "result_kind": "list",
                "warnings": [{"code": "INVALID_REQUEST", "message": "action must be one of find|list|count|exists"}],
                "items": [], "page": {"next_cursor": None, "truncated": False, "total_matches": 0}}

    if action in ("find", "list"):
        requested_limit = parse_int(call_args.get("limit", 5000), 5000, 1, 50000)
        requested_cursor = parse_cursor(call_args.get("cursor"))
        req = {k: v for k, v in call_args.items() if k != "cursor"}
        all_items, all_warnings, had_error = _aggregate_backends(
            clang_script, build_dir, workspace_root, targets, cmd, req, timeout_sec)
        items = dedupe_items(all_items)
        total = len(items)
        sliced = items[requested_cursor:requested_cursor + requested_limit]
        next_cursor = requested_cursor + len(sliced)
        truncated = next_cursor < total
        return {"status": "error" if had_error and not sliced else "ok", "result_kind": action,
                "items": sliced, "warnings": all_warnings,
                "page": {"next_cursor": str(next_cursor) if truncated else None, "truncated": truncated, "total_matches": total}}

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

    # exists
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


# ---------------------------------------------------------------------------
# MCP Server setup
# ---------------------------------------------------------------------------

def _tool_result(payload: dict[str, Any]) -> types.CallToolResult:
    is_error = payload.get("status") == "error"
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=json.dumps(payload, indent=2, ensure_ascii=True))],
        structuredContent=payload,
        isError=is_error,
    )


def create_server(tool_defs: list[dict[str, Any]], clang_script: Path,
                  workspace_root: Path, build_dir_arg: str | None,
                  backend_timeout: int) -> Server:
    server = Server(SERVER_NAME, SERVER_VERSION)
    _build_dir: str | None = None
    _source_files: list[str] | None = None

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return [
            types.Tool(
                name=t.get("name", ""),
                description=t.get("description", ""),
                inputSchema=t.get("inputSchema", {"type": "object"}),
            )
            for t in tool_defs
        ]

    @server.call_tool(validate_input=False)
    async def call_tool(name: str, arguments: dict[str, Any]) -> types.CallToolResult:
        nonlocal _build_dir, _source_files
        logger.info("call_tool %s  args=%s", name, json.dumps(arguments, sort_keys=True))

        if name not in VALID_TOOLS:
            return _tool_result(_backend_error("UNKNOWN_TOOL", f"unknown tool: {name}"))

        if _build_dir is None or _source_files is None:
            try:
                _build_dir, _source_files = resolve_runtime_context(workspace_root, build_dir_arg)
            except Exception as e:
                return _tool_result(_backend_error("RUNTIME_CONTEXT_ERROR", str(e)))

        payload = route_tool_call(clang_script, _build_dir, workspace_root, _source_files, name, arguments, backend_timeout)
        return _tool_result(payload)

    return server


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

async def amain(server: Server) -> None:
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace-root", default=".")
    parser.add_argument("--build-dir")
    parser.add_argument("--backend-timeout", type=int, default=12)
    parser.add_argument("--tools-json", default="tools.json")
    parser.add_argument("--clang-script", default="clang_mcp.py")
    parser.add_argument("-v", "--verbose", action="store_true", help="enable debug-level traces")
    args = parser.parse_args()

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        stream=sys.stderr,
    )

    base_dir = Path.cwd()
    workspace_root = norm_path(args.workspace_root, base_dir)
    tool_defs = load_tools_schema(norm_path(args.tools_json, workspace_root))
    clang_script = norm_path(args.clang_script, workspace_root)

    server = create_server(tool_defs, clang_script, workspace_root, args.build_dir, args.backend_timeout)
    anyio.run(amain, server)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
