#!/usr/bin/env bash
set -euo pipefail

# Regenerate all golden JSON outputs for CLI examples

EXAMPLES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_SCRIPT="$EXAMPLES_DIR/cli.sh"
EXPECTED_DIR="$EXAMPLES_DIR/expected"

# Define mapping of functions to expected filenames (same as in validate.py)
declare -A FUNC_TO_EXPECTED=(
    [example_doctor]="doctor_parse_ok"
    [example_semantic_list_functions_fields_limit]="semantic_list_functions_fields_limit"
    [example_semantic_count_calls_parse]="semantic_count_calls_parse"
    [example_semantic_exists_class_parse_false]="semantic_exists_class_parse_false"
    [example_semantic_find_loadfile]="semantic_find_loadfile"
    [example_semantic_list_calls_parser_fields]="semantic_list_calls_parser_fields"
    [example_semantic_file_entity_parse]="semantic_file_entity_parse"
    [example_semantic_exists_override_emitfromevents]="semantic_exists_override_emitfromevents"
    [example_semantic_count_classes_node]="semantic_count_classes_node"
)

# Create expected directory if missing
mkdir -p "$EXPECTED_DIR"

# Regenerate each golden file
for func_name in "${!FUNC_TO_EXPECTED[@]}"; do
    case_name="${FUNC_TO_EXPECTED[$func_name]}"
    expected_file="$EXPECTED_DIR/${case_name}.json"
    
    echo "Generating: $case_name ($func_name)"
    
    bash "$CLI_SCRIPT" "$func_name" 2>/dev/null | python3 -m json.tool > "$expected_file" || {
        echo "  ✗ FAILED to generate"
        rm -f "$expected_file"
        continue
    }
    
    echo "  ✓ Saved: $expected_file"
done

echo ""
echo "Golden files regenerated successfully!"
