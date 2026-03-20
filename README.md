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

The image also sets a default Git identity for commits created inside the
container. Override it at build time if needed:

```bash
GIT_USER_NAME="Andrey Chepurko" GIT_USER_EMAIL=achepurko1978@users.noreply.github.com docker compose build dev
```

When remote source changes, rebuild the image to pick up updates:

```bash
docker compose build dev
```

The MCP server is implemented in Python (`mcp_server.py`) and can call a C++
analysis back-end binary (Rust implementation in `clang_mcp_rs`).

---

## Python MCP Server

### Install dependencies

```bash
sudo apt-get update
sudo apt-get install -y python-is-python3 python3-clang python3-pip
python -m pip install --break-system-packages mcp pytest
```

The MCP server (`mcp_server.py`) requires the [MCP Python SDK](https://pypi.org/project/mcp/)
(`mcp` package, which includes `anyio`).

### Test MCP server locally

From the repository root:

```bash
# 1) Run MCP server unit tests
python -m pytest tests/test_mcp_server.py -q

# 2) Quick runtime sanity check (CLI starts)
python mcp_server.py --help
```

Expected test result: `91 passed`.

Optional: run all Python tests in the repo.

```bash
python -m pytest tests -q
```

### Prepare compile database

```bash
CC=clang CXX=clang++ cmake -S samples/cpp -B samples/cpp/build-rust-tests -D CMAKE_EXPORT_COMPILE_COMMANDS=ON
```

### Run

```bash
python mcp_server.py \
	--workspace-root /workspace \
	--build-dir /workspace/samples/cpp/build-rust-tests \
	--clang-script /workspace/clang_mcp_rs/target/release/clang_mcp
```

### Debug with MCP Inspector (Docker/dev container)

Install Inspector once:

```bash
sudo apt-get update
sudo apt-get install -y nodejs npm
npm install -g @modelcontextprotocol/inspector
```

Start Inspector on known-good ports (keep terminal open):

```bash
pkill -f mcp-inspector || true
HOST=0.0.0.0 SERVER_PORT=38139 CLIENT_PORT=42427 DANGEROUSLY_OMIT_AUTH=true mcp-inspector
```

Open the UI from inside container:

```bash
$BROWSER "http://127.0.0.1:42427/?MCP_PROXY_PORT=38139"
```

In Inspector UI, configure MCP server transport as `stdio` with:

- Command: `python`
- Args (copy/paste as JSON array):

```json
[
	"/workspace/mcp_server.py",
  "--workspace-root",
  "/workspace",
  "--build-dir",
  "/workspace/samples/cpp/build-rust-tests",
  "--clang-script",
  "/workspace/clang_mcp_rs/target/release/clang_mcp"
]
```

- Args (copy/paste as one line):

```bash
/workspace/mcp_server.py --workspace-root /workspace --build-dir /workspace/samples/cpp/build-rust-tests --clang-script /workspace/clang_mcp_rs/target/release/clang_mcp
```

Notes:

- Do not run Inspector under `timeout`; that terminates it.
- If you see `PORT IS IN USE`, pick another pair of free ports and restart Inspector.
- If browser still cannot connect, forward ports `42427` and `38139` in VS Code Ports view.

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

The Rust CLI is the back-end binary used by `mcp_server.py`:

```bash
# Health check
./clang_mcp_rs/target/release/clang_mcp --build-dir samples/cpp/build-rust-tests --file samples/cpp/src/parse.cpp doctor

# Resolve a symbol
./clang_mcp_rs/target/release/clang_mcp --build-dir samples/cpp/build-rust-tests --file samples/cpp/src/parse.cpp \
	cpp_resolve_symbol --request-json '{"name":"Load"}'

# Semantic query
./clang_mcp_rs/target/release/clang_mcp --build-dir samples/cpp/build-rust-tests --file samples/cpp/src/parser.cpp \
    cpp_semantic_query --request-json '{"action":"list","entity":"function"}'

# Describe a symbol
./clang_mcp_rs/target/release/clang_mcp --build-dir samples/cpp/build-rust-tests --file samples/cpp/include/yaml-cpp/exceptions.h \
	cpp_describe_symbol --request-json '{"symbol_id":"c:@N@YAML@S@BadConversion"}'
```

Requests can also be passed via `--request-file path/to/request.json`.

### Run tests

```bash
cd clang_mcp_rs
cargo test
```

### CLI examples with golden JSON outputs

Prebuilt sample cases for `samples/cpp` are stored in `samples/cli_examples`.
Each example is a real, copy-paste-ready CLI command.

Show all available copy-paste commands:

```bash
bash /workspace/samples/cli_examples/cli.sh

# Or run one specific example function
bash /workspace/samples/cli_examples/cli.sh example_doctor
```

Validate all examples against saved golden outputs:

```bash
bash /workspace/samples/cli_examples/cli.sh validate
```

Refresh golden outputs after intentional behavior changes:

```bash
bash /workspace/samples/cli_examples/regenerate_golden.sh
```

### Raw MCP JSON-RPC tool calls (via `mcp_server.py`)

If you want to test MCP server tool calls directly with JSON arguments over stdio,
use the helper script:

```bash
# Show usage
bash /workspace/samples/cli_examples/mcp_raw_tool_call.sh --help

# Shorthand form: tool name + JSON args
bash /workspace/samples/cli_examples/mcp_raw_tool_call.sh cpp_semantic_query '{"action":"list","entity":"function","scope":{"path":"samples/cpp/src/parse.cpp"},"where":{"name":"Load"},"limit":5}'

# Full form: workspace_root build_dir clang_script tool_name JSON args
bash /workspace/samples/cli_examples/mcp_raw_tool_call.sh /workspace /workspace/samples/cpp/build-rust-tests /workspace/clang_mcp_rs/target/release/clang_mcp cpp_resolve_symbol '{"name":"Load"}'

# Read arguments JSON from a file
bash /workspace/samples/cli_examples/mcp_raw_tool_call.sh cpp_semantic_query @/workspace/request.json

# Use bundled sample request JSON
bash /workspace/samples/cli_examples/mcp_raw_tool_call.sh cpp_semantic_query @/workspace/samples/cli_examples/request.semantic.list.load.json
```

Bundled sample request file:

- `/workspace/samples/cli_examples/request.semantic.list.load.json`

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
			"command": "python",
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
			"command": "python",
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

### MCP server

- `mcp_server.py`
- `tools.json`

### Rust back-end (recommended)

- `clang_mcp_rs/target/release/clang_mcp` (pre-built binary)

### Common requirements

Required Python package (for `mcp_server.py`):

- `mcp` (`python -m pip install --break-system-packages mcp`)

Required generated file (from your CMake configure step):

- `build/compile_commands.json` (or any discovered compile_commands.json in the workspace)

Required VS Code config (choose one):

- Workspace config: `.vscode/mcp.json`
- Or user/global MCP config that points to `mcp_server.py`

Not required for deployment:

- `sample.cpp`
- `cpp-mcp-v1.schema.json` (useful for documentation/reference, but not required at runtime by this server)
