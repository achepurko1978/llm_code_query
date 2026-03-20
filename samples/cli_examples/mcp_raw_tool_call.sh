#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    cat <<'EOF'
Usage:
    mcp_raw_tool_call.sh [workspace_root] [build_dir] [clang_script] [tool_name] [args_json_or_@file]
    mcp_raw_tool_call.sh [tool_name] [args_json_or_@file]

Defaults:
    workspace_root: /workspace
    build_dir:      /workspace/samples/cpp/build-rust-tests
    clang_script:   /workspace/clang_mcp_rs/target/release/clang_mcp
    tool_name:      cpp_semantic_query

Examples:
    mcp_raw_tool_call.sh
    mcp_raw_tool_call.sh cpp_semantic_query '{"action":"list","entity":"function","scope":{"path":"samples/cpp/src/parse.cpp"},"where":{"name":"Load"},"limit":5}'
    mcp_raw_tool_call.sh /workspace /workspace/samples/cpp/build-rust-tests /workspace/clang_mcp_rs/target/release/clang_mcp cpp_semantic_query '{"action":"list","entity":"function","scope":{"path":"samples/cpp/src/parse.cpp"}}'
    mcp_raw_tool_call.sh /workspace /workspace/samples/cpp/build-rust-tests /workspace/clang_mcp_rs/target/release/clang_mcp cpp_semantic_query @/workspace/request.json
EOF
    exit 0
fi

is_tool_name() {
    case "$1" in
        cpp_semantic_query) return 0 ;;
        *) return 1 ;;
    esac
}

if [[ -n "${1:-}" ]] && is_tool_name "$1"; then
    TOOL_NAME="$1"
    ARGS_JSON="${2:-{\"action\":\"list\",\"entity\":\"function\",\"scope\":{\"path\":\"samples/cpp/src/parse.cpp\"},\"where\":{\"name\":\"Load\"},\"fields\":[\"symbol_id\",\"qualified_name\"],\"limit\":5}}"
    WORKSPACE_ROOT="/workspace"
    BUILD_DIR="$WORKSPACE_ROOT/samples/cpp/build-rust-tests"
    CLANG_SCRIPT="$WORKSPACE_ROOT/clang_mcp_rs/target/release/clang_mcp"
else
    WORKSPACE_ROOT="${1:-/workspace}"
    BUILD_DIR="${2:-$WORKSPACE_ROOT/samples/cpp/build-rust-tests}"
    CLANG_SCRIPT="${3:-$WORKSPACE_ROOT/clang_mcp_rs/target/release/clang_mcp}"
    TOOL_NAME="${4:-cpp_semantic_query}"
    ARGS_JSON="${5:-{\"action\":\"list\",\"entity\":\"function\",\"scope\":{\"path\":\"samples/cpp/src/parse.cpp\"},\"where\":{\"name\":\"Load\"},\"fields\":[\"symbol_id\",\"qualified_name\"],\"limit\":5}}"
fi

if [[ ! -f "$BUILD_DIR/compile_commands.json" ]]; then
  cmake -S "$WORKSPACE_ROOT/samples/cpp" -B "$BUILD_DIR" -D CMAKE_EXPORT_COMPILE_COMMANDS=ON >/dev/null
fi

if [[ ! -x "$CLANG_SCRIPT" ]]; then
  echo "clang backend not found or not executable: $CLANG_SCRIPT" >&2
  exit 1
fi

python3 - "$WORKSPACE_ROOT" "$BUILD_DIR" "$CLANG_SCRIPT" "$TOOL_NAME" "$ARGS_JSON" <<'PY'
import json
import re
import subprocess
import sys
from pathlib import Path

workspace_root, build_dir, clang_script, tool_name, args_json = sys.argv[1:6]


def parse_args_json(raw: str) -> dict:
    text = raw.strip()
    if text.startswith("@"):
        raw_path = text[1:]
        file_path = Path(raw_path)

        # Be forgiving if extra characters were accidentally appended after the file path.
        if not file_path.is_file():
            m = re.match(r"^(.*?\.json)\b", raw_path)
            if m:
                candidate = Path(m.group(1))
                if candidate.is_file():
                    file_path = candidate

        if not file_path.is_file():
            raise FileNotFoundError(
                f"arguments file not found: {raw_path}. Use @/absolute/path/to/request.json"
            )

        text = file_path.read_text(encoding="utf-8").strip()

    decoder = json.JSONDecoder()
    obj, idx = decoder.raw_decode(text)
    if not isinstance(obj, dict):
        raise ValueError("tool arguments must decode to a JSON object")

    trailing = text[idx:].strip()
    if trailing:
        # Some shell wrappers can leak trailing characters; keep the first valid JSON object.
        pass
    return obj

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
        "name": tool_name,
        "arguments": parse_args_json(args_json),
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
