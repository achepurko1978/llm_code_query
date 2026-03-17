#!/usr/bin/env python3
"""Minimal MCP server for debugging stdin/stdout pipe issues."""
import os
import json
import select

def log(msg):
    os.write(2, f"[mini-mcp] {msg}\n".encode())

log("started")

# Check if stdin has data
ready, _, _ = select.select([0], [], [], 5.0)
log(f"select ready={bool(ready)}")

if ready:
    chunk = os.read(0, 4096)
    log(f"os.read got {len(chunk)} bytes: {chunk[:120]!r}")
else:
    log("no data on fd 0 after 5s, trying sys.stdin.buffer")
    import sys
    ready2, _, _ = select.select([sys.stdin.buffer], [], [], 2.0)
    log(f"select on sys.stdin.buffer ready={bool(ready2)}")
    peek = sys.stdin.buffer.peek(1)
    log(f"peek got {len(peek)} bytes: {peek[:80]!r}")

# Try to read the full initialize request
def read_line():
    buf = b""
    while True:
        ch = os.read(0, 1)
        if not ch:
            return None
        buf += ch
        if buf.endswith(b"\n"):
            return buf.decode()

def read_msg():
    headers = {}
    while True:
        line = read_line()
        if line is None:
            return None
        if line.strip() == "":
            break
        if ":" in line:
            k, v = line.split(":", 1)
            headers[k.strip().lower()] = v.strip()
    cl = int(headers.get("content-length", "0"))
    if cl <= 0:
        return None
    body = b""
    while len(body) < cl:
        chunk = os.read(0, cl - len(body))
        if not chunk:
            return None
        body += chunk
    return json.loads(body)

def write_msg(payload):
    raw = json.dumps(payload).encode()
    header = f"Content-Length: {len(raw)}\r\n\r\n".encode()
    os.write(1, header + raw)

log("reading message...")
msg = read_msg()
log(f"got: {msg}")

if msg and msg.get("method") == "initialize":
    resp = {
        "jsonrpc": "2.0",
        "id": msg.get("id"),
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "mini-mcp", "version": "0.1.0"},
        },
    }
    write_msg(resp)
    log("sent initialize response")

# Keep running
while True:
    msg = read_msg()
    if msg is None:
        log("EOF, exiting")
        break
    method = msg.get("method", "")
    mid = msg.get("id")
    log(f"got method={method} id={mid}")
    if method == "tools/list" and mid is not None:
        write_msg({"jsonrpc": "2.0", "id": mid, "result": {"tools": []}})
    elif method == "notifications/initialized":
        pass
    elif mid is not None:
        write_msg({"jsonrpc": "2.0", "id": mid, "result": {}})
