# CLI Examples for clang_mcp

**Copy & paste ready.** No thinking, no setup, no functions—just copy a command line and paste it into your terminal.

## Quick Start

Show all 14 copy-paste commands:
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

All 14 commands work the same way.

## Commands

- **1-4**: Various `doctor` and `cpp_resolve_symbol` examples
- **5-12**: `cpp_semantic_query` examples (list, count, exists, find)
- **13-14**: `cpp_describe_symbol` examples

## Validation

Verify all examples still work:
```bash
bash /workspace/samples/cli_examples/cli.sh validate
```

Expected: `Results: 14 passed, 0 failed`

## Files

- **cli.sh** — Main script with 14 copy-paste commands + validation
- **validate.py** — Python validator (for regression testing)
- **regenerate_golden.sh** — Refresh golden outputs after changes
- **expected/** — 14 golden JSON snapshots (for validation)
