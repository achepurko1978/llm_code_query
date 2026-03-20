# CLI Examples for clang_mcp

**Copy & paste ready.** No thinking, no setup, no functions—just copy a command line and paste it into your terminal.

## Quick Start

Show all copy-paste commands:
```bash
bash /workspace/samples/cli_examples/cli.sh
```

Then pick any command, copy it, and paste into your terminal. That's it.

## Example

This is what you see:
```
1. DOCTOR (health check)


/workspace/clang_mcp_rs/target/debug/clang_mcp \
  --build-dir /workspace/samples/cpp/build-rust-tests \
  --file /workspace/samples/cpp/src/parse.cpp doctor
```

Just copy the command → paste into terminal:
```bash
/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp doctor
```

## Commands

- **1**: `doctor` health check
- **2-9**: `cpp_semantic_query` examples (list, count, exists, find)

## Validation

Verify all examples still work:
```bash
bash /workspace/samples/cli_examples/cli.sh validate
```

## Files

- **cli.sh** — Main script with copy-paste commands + validation
- **validate.py** — Python validator (for regression testing)
- **regenerate_golden.sh** — Refresh golden outputs after changes
- **expected/** — Golden JSON snapshots (for validation)
