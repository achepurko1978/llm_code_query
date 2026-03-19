# clang_mcp Quickstart

## Docker (Repo Baked Into Image)

The Docker setup now clones the repository directly from GitHub at build time
and does not use a bind mount.

```bash
docker compose build dev
docker compose run --rm dev bash
```

You can override the repository URL and revision (branch, tag, or commit):

```bash
GIT_REPO_URL=https://github.com/achepurko1978/llm_code_query.git GIT_REF=main docker compose build dev
```

When remote source changes, rebuild the image to pick up updates:

```bash
docker compose build dev
```

Two back-end implementations are available — the original Python script and a
Rust rewrite that produces identical output but compiles to a single native
binary.

---

## Python Back-End

### Install dependencies

```bash
sudo apt-get update
sudo apt-get install -y python3-clang
pip install mcp
```

The MCP server (`mcp_server.py`) requires the [MCP Python SDK](https://pypi.org/project/mcp/)
(`mcp` package, which includes `anyio`). The clang back-end (`clang_mcp.py`)
requires `python3-clang`.

### Prepare compile database

```bash
CC=clang CXX=clang++ cmake -S . -B build
```

### Run

```bash
python3 clang_mcp.py doctor
python3 clang_mcp.py --build-dir build --file sample.cpp cpp_resolve_symbol --request-json '{"name":"add"}'
python3 clang_mcp.py --build-dir build --file sample.cpp cpp_semantic_query --request-json '{"action":"list","entity":"function"}'
python3 clang_mcp.py --build-dir build --file sample.cpp cpp_describe_symbol --request-json '{"symbol_id":"c:@F@add#I#I#"}'
```

---

## Rust Back-End

### Prerequisites

| Dependency | Install |
|---|---|
| Rust toolchain (≥ 1.75) | `sudo apt-get install -y rustc cargo` (or [rustup](https://rustup.rs)) |
| libclang dev headers | already provided by `libclang-*-dev` in the Docker image |

### Build

```bash
cd clang_mcp_rs
cargo build --release
```

The binary is produced at `clang_mcp_rs/target/release/clang_mcp`.

> **Tip — corporate proxy:** If `cargo` fails with TLS/certificate errors behind
> a corporate proxy (e.g. Zscaler), set `CARGO_HTTP_CHECK_REVOKE=false`.

### Run

The CLI is a drop-in replacement for the Python script:

```bash
# Health check
./clang_mcp_rs/target/release/clang_mcp --build-dir build --file sample.cpp doctor

# Resolve a symbol
./clang_mcp_rs/target/release/clang_mcp --build-dir build --file sample.cpp \
    cpp_resolve_symbol --request-json '{"name":"add"}'

# Semantic query
./clang_mcp_rs/target/release/clang_mcp --build-dir build --file sample.cpp \
    cpp_semantic_query --request-json '{"action":"list","entity":"function"}'

# Describe a symbol
./clang_mcp_rs/target/release/clang_mcp --build-dir build --file sample.cpp \
    cpp_describe_symbol --request-json '{"symbol_id":"c:@F@add#I#I#"}'
```

Requests can also be passed via `--request-file path/to/request.json`.

### Run tests

```bash
cd clang_mcp_rs
cargo test
```

### Install

```bash
cargo install --path . --root .
```

---

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

### Using the Rust back-end with the MCP server

The MCP server shells out to a back-end script for each tool call.
Pass `--clang-script` to point it at the Rust binary instead of the Python
script:

```jsonc
// .vscode/mcp.json — Rust back-end variant
{
	"servers": {
		"clang-cpp": {
			"type": "stdio",
			"command": "python3",
			"args": [
				"${workspaceFolder}/mcp_server.py",
				"--workspace-root", "${workspaceFolder}",
				"--backend-timeout", "12",
				"--clang-script", "${workspaceFolder}/clang_mcp_rs/target/release/clang_mcp"
			]
		}
	}
}
```

Everything else (tool names, request/response format) is identical.

## Minimum Files To Deploy

### Python back-end

- `mcp_server.py`
- `clang_mcp.py`
- `tools.json`

### Rust back-end

- `mcp_server.py`
- `clang_mcp_rs/target/release/clang_mcp` (pre-built binary)
- `tools.json`

### Common requirements

Required Python package (for `mcp_server.py`):

- `mcp` (`pip install mcp`)

Required generated file (from your CMake configure step):

- `build/compile_commands.json` (or any discovered compile_commands.json in the workspace)

Required VS Code config (choose one):

- Workspace config: `.vscode/mcp.json`
- Or user/global MCP config that points to `mcp_server.py`

Not required for deployment:

- `sample.cpp`
- `cpp-mcp-v1.schema.json` (useful for documentation/reference, but not required at runtime by this server)
