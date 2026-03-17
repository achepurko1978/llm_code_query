# clang_mcp.py Quickstart

## Install dependency

```bash
sudo apt-get update
sudo apt-get install -y python3-clang
```

## Prepare compile database

```bash
CC=clang CXX=clang++ cmake -S . -B build
```

## Run

```bash
python3 clang_mcp.py doctor
python3 clang_mcp.py --build-dir build --file sample.cpp cpp_resolve_symbol --request-json '{"name":"add"}'
python3 clang_mcp.py --build-dir build --file sample.cpp cpp_semantic_query --request-json '{"action":"list","entity":"function"}'
python3 clang_mcp.py --build-dir build --file sample.cpp cpp_describe_symbol --request-json '{"symbol_id":"c:@F@add#I#I#"}'
```

## Use With Copilot (Any C++ Workspace)

1. Generate compile commands in your workspace (required):

```bash
CC=clang CXX=clang++ cmake -S . -B build
```

2. Add `.vscode/mcp.json`:

```json
{
	"servers": {
		"clang-cpp": {
			"type": "stdio",
			"command": "python3",
			"args": [
				"${workspaceFolder}/mcp_server.py",
				"--workspace-root",
				"${workspaceFolder}",
				"--backend-timeout",
				"12"
			]
		}
	}
}
```

3. In VS Code: run `MCP: List Servers`, start `clang-cpp`, trust it, then enable its tools in Chat.

### Let Copilot Choose Tools Automatically

You do not need to type tool names in prompts.

1. Make sure `clang-cpp` is started and tools are enabled in Chat.
2. Ask in natural language, for example:
	- "Find where `add` is defined and summarize overloads."
	- "List all functions in this C++ workspace."
	- "Describe this symbol: c:@N@fun@F@add#I#I#."
3. Copilot will decide whether to call MCP tools.

Tips for best automatic tool selection:

- Use Agent mode in Chat (not plain Ask mode).
- Mention intent and scope (workspace/file/symbol), not tool names.
- Keep `clang-cpp` tools enabled in Configure Tools.

## Minimum Files To Deploy

Required files in your workspace:

- `mcp_server.py`
- `clang_mcp.py`
- `tools.json`

Required generated file (from your CMake configure step):

- `build/compile_commands.json` (or any discovered compile_commands.json in the workspace)

Required VS Code config (choose one):

- Workspace config: `.vscode/mcp.json`
- Or user/global MCP config that points to `mcp_server.py`

Not required for deployment:

- `samples/` folder
- `sample.cpp`
- `cpp-mcp-v1.schema.json` (useful for documentation/reference, but not required at runtime by this server)
