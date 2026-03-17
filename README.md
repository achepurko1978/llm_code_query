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
