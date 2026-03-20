#!/usr/bin/env bash
set -euo pipefail

WORKSPACE_ROOT="${1:-/workspace}"
BUILD_DIR="${2:-$WORKSPACE_ROOT/samples/cpp/build-rust-tests}"
CLANG_SCRIPT="${3:-$WORKSPACE_ROOT/clang_mcp_rs/target/release/clang_mcp}"
REQUEST_JSON="${4:-{\"action\":\"list\",\"entity\":\"function\",\"scope\":{\"path\":\"samples/cpp/src/parse.cpp\"},\"where\":{\"name\":\"Load\"},\"fields\":[\"symbol_id\",\"qualified_name\"],\"limit\":5}}"

if [[ ! -f "$BUILD_DIR/compile_commands.json" ]]; then
  cmake -S "$WORKSPACE_ROOT/samples/cpp" -B "$BUILD_DIR" -D CMAKE_EXPORT_COMPILE_COMMANDS=ON >/dev/null
fi

if [[ ! -x "$CLANG_SCRIPT" ]]; then
  echo "clang backend not found or not executable: $CLANG_SCRIPT" >&2
  exit 1
fi

python3 - "$WORKSPACE_ROOT" "$BUILD_DIR" "$CLANG_SCRIPT" "$REQUEST_JSON" <<'PY'
import json
import subprocess
import sys

workspace_root, build_dir, clang_script, request_json = sys.argv[1:5]

cmd = [
    sys.executable,
    f"{workspace_root}/mcp_server.py",
    "--workspace-root",
    workspace_root,
    "--build-dir",
    build_dir,
    "--clang-script",
    clang_script,
]

proc = subprocess.Popen(
    cmd,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)


def send_message(msg: dict) -> None:
    body = json.dumps(msg, separators=(",", ":")) + "\n"
    assert proc.stdin is not None
    proc.stdin.write(body.encode("utf-8"))
    proc.stdin.flush()


def read_message() -> dict:
    assert proc.stdout is not None
    line = proc.stdout.readline()
    if not line:
        raise RuntimeError("Unexpected EOF while reading MCP response")
    return json.loads(line.decode("utf-8"))


init_req = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "raw-cli", "version": "1.0"},
    },
}

initialized_notification = {
    "jsonrpc": "2.0",
    "method": "notifications/initialized",
}

tool_call_req = {
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
        "name": "cpp_semantic_query",
        "arguments": json.loads(request_json),
    },
}

try:
    send_message(init_req)
    init_resp = read_message()

    send_message(initialized_notification)
    send_message(tool_call_req)
    call_resp = read_message()

    print(json.dumps({"initialize": init_resp, "tool_call": call_resp}, indent=2))
finally:
    try:
        proc.terminate()
    except Exception:
        pass
PY
