#!/usr/bin/env python3
"""
Validate CLI examples against golden JSON outputs.
Run after sourcing cli.sh functions.
"""

import json
import subprocess
import sys
from pathlib import Path

# Map function names to golden output filenames
FUNC_TO_EXPECTED = {
    "example_doctor": "doctor_parse_ok",
    "example_semantic_list_functions_fields_limit": "semantic_list_functions_fields_limit",
    "example_semantic_count_calls_parse": "semantic_count_calls_parse",
    "example_semantic_exists_class_parse_false": "semantic_exists_class_parse_false",
    "example_semantic_find_loadfile": "semantic_find_loadfile",
    "example_semantic_list_calls_parser_fields": "semantic_list_calls_parser_fields",
    "example_semantic_file_entity_parse": "semantic_file_entity_parse",
    "example_semantic_exists_override_emitfromevents": "semantic_exists_override_emitfromevents",
    "example_semantic_count_classes_node": "semantic_count_classes_node",
}

EXAMPLES_DIR = Path(__file__).parent
BUILD_DIR = Path("/workspace/samples/cpp/build-rust-tests")

def run_example(func_name):
    """Run an example function and return output."""
    script_dir = EXAMPLES_DIR
    cli_script = script_dir / "cli.sh"
    
    try:
        result = subprocess.run(
            ["bash", str(cli_script), func_name],
            capture_output=True,
            text=True,
            timeout=10
        )
        return result.stdout.strip()
    except subprocess.TimeoutExpired:
        return None
    except Exception as e:
        print(f"Error running {func_name}: {e}", file=sys.stderr)
        return None

def load_json_safe(text):
    """Load JSON or return None on failure."""
    if not text:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return None

def main():
    passed = 0
    failed = 0
    
    # Ensure build directory exists
    if not BUILD_DIR.exists():
        print(f"ERROR: Build directory {BUILD_DIR} does not exist")
        sys.exit(1)
    
    for func_name, case_name in sorted(FUNC_TO_EXPECTED.items()):
        expected_file = EXAMPLES_DIR / "expected" / f"{case_name}.json"
        
        if not expected_file.exists():
            print(f"⚠ MISSING  {case_name} (expected file: {expected_file})")
            failed += 1
            continue
        
        # Load expected
        try:
            with open(expected_file) as f:
                expected = json.load(f)
        except json.JSONDecodeError as e:
            print(f"✗ FAIL    {case_name} (invalid expected JSON: {e})")
            failed += 1
            continue
        
        # Run example
        actual_text = run_example(func_name)
        actual = load_json_safe(actual_text)
        
        if actual is None:
            print(f"✗ FAIL    {case_name} (no output or invalid JSON)")
            failed += 1
            continue
        
        # Compare
        if actual == expected:
            print(f"✓ PASS    {case_name}")
            passed += 1
        else:
            print(f"✗ FAIL    {case_name} (output mismatch)")
            failed += 1
    
    print(f"\nResults: {passed} passed, {failed} failed")
    
    sys.exit(0 if failed == 0 else 1)

if __name__ == "__main__":
    main()
