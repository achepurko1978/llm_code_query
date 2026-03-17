#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

CC=clang CXX=clang++ cmake -S . -B build
cmake --build build -j

python3 clang_mcp.py --build-dir build --file samples/cpp/functions.cpp cpp_resolve_symbol --request-file samples/requests/resolve_add.request.json > samples/responses/functions.resolve_add.response.json
python3 clang_mcp.py --build-dir build --file samples/cpp/functions.cpp cpp_semantic_query --request-file samples/requests/semantic_functions_list.request.json > samples/responses/functions.semantic_functions_list.response.json
python3 clang_mcp.py --build-dir build --file samples/cpp/functions.cpp cpp_semantic_query --request-file samples/requests/semantic_calls_list.request.json > samples/responses/functions.semantic_calls_list.response.json

python3 clang_mcp.py --build-dir build --file samples/cpp/classes.cpp cpp_semantic_query --request-file samples/requests/semantic_methods_list.request.json > samples/responses/classes.semantic_methods_list.response.json
python3 clang_mcp.py --build-dir build --file samples/cpp/classes.cpp cpp_semantic_query --request-file samples/requests/semantic_exists_override.request.json > samples/responses/classes.semantic_exists_override.response.json
python3 clang_mcp.py --build-dir build --file samples/cpp/classes.cpp cpp_semantic_query --request-file samples/requests/semantic_exists_virtual.request.json > samples/responses/classes.semantic_exists_virtual.response.json

python3 clang_mcp.py --build-dir build --file samples/cpp/data.cpp cpp_semantic_query --request-file samples/requests/semantic_structs_list.request.json > samples/responses/data.semantic_structs_list.response.json

SID=$(python3 clang_mcp.py --build-dir build --file samples/cpp/functions.cpp cpp_resolve_symbol --request-file samples/requests/resolve_add.request.json | python3 -c 'import sys,json; print(json.load(sys.stdin)["items"][0]["symbol_id"])')
printf '{\n  "symbol_id": "%s",\n  "include_relations": true,\n  "relation_limit": 20\n}\n' "$SID" > samples/requests/describe_add.request.json
python3 clang_mcp.py --build-dir build --file samples/cpp/functions.cpp cpp_describe_symbol --request-file samples/requests/describe_add.request.json > samples/responses/functions.describe_add.response.json

echo "Sample requests executed successfully. Responses are in samples/responses/."
