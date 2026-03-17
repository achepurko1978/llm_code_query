#!/bin/bash
LOG="/tmp/mcp_wrapper.log"
echo "=== MCP wrapper started at $(date) ===" >> "$LOG"
echo "pwd: $(pwd)" >> "$LOG"
echo "python3: $(which python3)" >> "$LOG"
echo "args: $@" >> "$LOG"

exec python3 -u "$@" 2>> "$LOG"
