#!/usr/bin/env bash
# CLI Examples for clang_mcp—directly copy & paste ready
# Run to see all commands: bash cli.sh
# Validate: bash cli.sh validate
# Source for functions: source cli.sh

set -euo pipefail

BUILD_DIR="/workspace/samples/cpp/build-rust-tests"
CPP_ROOT="/workspace/samples/cpp"
CLANG_MCP="/workspace/clang_mcp_rs/target/debug/clang_mcp"
EXAMPLES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Ensure build directory exists
ensure_build() {
    if [[ ! -f "$BUILD_DIR/compile_commands.json" ]]; then
        cmake -S "$CPP_ROOT" -B "$BUILD_DIR" -G Ninja \
            -D CMAKE_CXX_COMPILER=clang++ \
            -D CMAKE_EXPORT_COMPILE_COMMANDS=ON > /dev/null 2>&1
    fi
}

# ============================================================================
# BASH FUNCTIONS (for sourcing and validation)
# ============================================================================

example_doctor() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp doctor
}

example_resolve_load_ambiguous() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_resolve_symbol --request-json '{"name": "Load"}'
}

example_resolve_missing_name_error() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_resolve_symbol --request-json '{}'
}

example_resolve_loadfile_function() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_resolve_symbol --request-json '{"name": "parse", "entity": "function"}'
}

example_semantic_list_functions_fields_limit() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parser.cpp cpp_semantic_query --request-json '{"action": "list", "entity": "function", "fields": ["name", "qualified_name"], "limit": 3}'
}

example_semantic_count_calls_parse() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parser.cpp cpp_semantic_query --request-json '{"action": "count", "entity": "call"}'
}

example_semantic_exists_class_parse_false() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_semantic_query --request-json '{"action": "exists", "entity": "class"}'
}

example_semantic_find_loadfile() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_semantic_query --request-json '{"action": "find", "entity": "function", "where": {"name": "parse"}}'
}

example_semantic_list_calls_parser_fields() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parser.cpp cpp_semantic_query --request-json '{"action": "list", "entity": "call", "fields": ["name"], "limit": 5}'
}

example_semantic_file_entity_parse() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_semantic_query --request-json '{"action": "list", "entity": "file"}'
}

example_semantic_exists_override_emitfromevents() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/emitterstate.cpp cpp_semantic_query --request-json '{"action": "exists", "entity": "method", "where": {"override": true}}'
}

example_semantic_count_classes_node() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/include/yaml-cpp/emitter.h cpp_semantic_query --request-json '{"action": "count", "entity": "class"}'
}

example_describe_badconversion() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/include/yaml-cpp/exceptions.h cpp_describe_symbol --request-json '{"symbol_id": "c:@N@YAML@S@BadConversion", "include_relations": true}'
}

example_describe_missing_symbol() {
    /workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_describe_symbol --request-json '{"symbol_id": "missing.symbol.id"}'
}

# ============================================================================
# DISPLAY (for copy-paste)
# ============================================================================

show_commands() {
    cat << 'EOF'
╔════════════════════════════════════════════════════════════════════════════╗
║                  Copy & Paste CLI Examples for clang_mcp                  ║
║                                                                            ║
║         Pick any command below, copy it, and paste into terminal          ║
╚════════════════════════════════════════════════════════════════════════════╝

1. DOCTOR (health check)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp doctor


2. RESOLVE: Ambiguous "Load" symbol
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_resolve_symbol --request-json '{"name": "Load"}'


3. RESOLVE: Empty request (error case)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_resolve_symbol --request-json '{}'


4. RESOLVE: Specific function "parse"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_resolve_symbol --request-json '{"name": "parse", "entity": "function"}'


5. SEMANTIC: List functions with fields and limit
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parser.cpp cpp_semantic_query --request-json '{"action": "list", "entity": "function", "fields": ["name", "qualified_name"], "limit": 3}'


6. SEMANTIC: Count calls
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parser.cpp cpp_semantic_query --request-json '{"action": "count", "entity": "call"}'


7. SEMANTIC: Check if class exists (false)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_semantic_query --request-json '{"action": "exists", "entity": "class"}'


8. SEMANTIC: Find "parse" function
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_semantic_query --request-json '{"action": "find", "entity": "function", "where": {"name": "parse"}}'


9. SEMANTIC: List calls with field filtering
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parser.cpp cpp_semantic_query --request-json '{"action": "list", "entity": "call", "fields": ["name"], "limit": 5}'


10. SEMANTIC: List file entities
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_semantic_query --request-json '{"action": "list", "entity": "file"}'


11. SEMANTIC: Check for override methods
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/emitterstate.cpp cpp_semantic_query --request-json '{"action": "exists", "entity": "method", "where": {"override": true}}'


12. SEMANTIC: Count classes in emitter.h
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/include/yaml-cpp/emitter.h cpp_semantic_query --request-json '{"action": "count", "entity": "class"}'


13. DESCRIBE: Symbol with relations
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/include/yaml-cpp/exceptions.h cpp_describe_symbol --request-json '{"symbol_id": "c:@N@YAML@S@BadConversion", "include_relations": true}'


14. DESCRIBE: Missing symbol (error case)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/workspace/clang_mcp_rs/target/debug/clang_mcp --build-dir /workspace/samples/cpp/build-rust-tests --file /workspace/samples/cpp/src/parse.cpp cpp_describe_symbol --request-json '{"symbol_id": "missing.symbol.id"}'


══════════════════════════════════════════════════════════════════════════════

✓ Just copy any complete command line above and paste into your terminal
✓ No sourcing needed
✓ No thinking required—it just works

EOF
}

validate_all() {
    ensure_build
    python3 "$EXAMPLES_DIR/validate.py"
}

# ============================================================================
# MAIN
# ============================================================================

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    if [[ "${1:-}" == "validate" ]]; then
        validate_all
    elif [[ -n "${1:-}" ]]; then
        # If argument is a function name, call it
        if declare -f "$1" > /dev/null; then
            "$1"
        else
            show_commands
        fi
    else
        show_commands
    fi
fi
