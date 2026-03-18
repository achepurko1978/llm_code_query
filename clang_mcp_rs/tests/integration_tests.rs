/// Integration tests: run the binary against sample files and compare JSON output.
use assert_cmd::Command;
use serde_json::Value;
use std::fs;

/// Path to the workspace root (from which build/ and samples/ are accessible).
fn workspace_root() -> String {
    // The binary is run from workspace root
    "/workspace".to_string()
}

fn run_tool(file: &str, tool: &str, request_file: &str) -> Value {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir",
            &format!("{ws}/build"),
            "--file",
            &format!("{ws}/samples/cpp/{file}"),
            tool,
            "--request-file",
            &format!("{ws}/samples/requests/{request_file}"),
        ])
        .output()
        .expect("failed to execute binary");
    assert!(output.status.success(), "binary exited with error: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("invalid JSON output")
}

fn load_expected(response_file: &str) -> Value {
    let ws = workspace_root();
    let content = fs::read_to_string(format!("{ws}/samples/responses/{response_file}"))
        .unwrap_or_else(|_| panic!("missing response file: {response_file}"));
    serde_json::from_str(&content).expect("invalid JSON in response file")
}

fn assert_json_eq(actual: &Value, expected: &Value, label: &str) {
    // Compare via sorted JSON to ignore field ordering differences
    let a = serde_json::to_string(&sort_json(actual)).unwrap();
    let b = serde_json::to_string(&sort_json(expected)).unwrap();
    assert_eq!(a, b, "JSON mismatch for {label}:\nactual:   {a}\nexpected: {b}");
}

fn sort_json(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut sorted: Vec<(String, Value)> = m
                .iter()
                .map(|(k, v)| (k.clone(), sort_json(v)))
                .collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(sort_json).collect()),
        other => other.clone(),
    }
}

// -------------------------------------------------------------------
// Sample response tests
// -------------------------------------------------------------------

#[test]
fn test_resolve_add() {
    let actual = run_tool("functions.cpp", "cpp_resolve_symbol", "resolve_add.request.json");
    let expected = load_expected("functions.resolve_add.response.json");
    assert_json_eq(&actual, &expected, "resolve_add");
}

#[test]
fn test_semantic_functions_list() {
    let actual = run_tool(
        "functions.cpp",
        "cpp_semantic_query",
        "semantic_functions_list.request.json",
    );
    let expected = load_expected("functions.semantic_functions_list.response.json");
    assert_json_eq(&actual, &expected, "semantic_functions_list");
}

#[test]
fn test_semantic_calls_list() {
    let actual = run_tool(
        "functions.cpp",
        "cpp_semantic_query",
        "semantic_calls_list.request.json",
    );
    let expected = load_expected("functions.semantic_calls_list.response.json");
    assert_json_eq(&actual, &expected, "semantic_calls_list");
}

#[test]
fn test_semantic_methods_list() {
    let actual = run_tool(
        "classes.cpp",
        "cpp_semantic_query",
        "semantic_methods_list.request.json",
    );
    let expected = load_expected("classes.semantic_methods_list.response.json");
    assert_json_eq(&actual, &expected, "semantic_methods_list");
}

#[test]
fn test_semantic_exists_override() {
    let actual = run_tool(
        "classes.cpp",
        "cpp_semantic_query",
        "semantic_exists_override.request.json",
    );
    let expected = load_expected("classes.semantic_exists_override.response.json");
    assert_json_eq(&actual, &expected, "semantic_exists_override");
}

#[test]
fn test_semantic_exists_virtual() {
    let actual = run_tool(
        "classes.cpp",
        "cpp_semantic_query",
        "semantic_exists_virtual.request.json",
    );
    let expected = load_expected("classes.semantic_exists_virtual.response.json");
    assert_json_eq(&actual, &expected, "semantic_exists_virtual");
}

#[test]
fn test_semantic_structs_list() {
    let actual = run_tool(
        "data.cpp",
        "cpp_semantic_query",
        "semantic_structs_list.request.json",
    );
    let expected = load_expected("data.semantic_structs_list.response.json");
    assert_json_eq(&actual, &expected, "semantic_structs_list");
}

#[test]
fn test_describe_add() {
    let actual = run_tool(
        "functions.cpp",
        "cpp_describe_symbol",
        "describe_add.request.json",
    );
    let expected = load_expected("functions.describe_add.response.json");
    assert_json_eq(&actual, &expected, "describe_add");
}

// -------------------------------------------------------------------
// Edge case / error handling tests
// -------------------------------------------------------------------

#[test]
fn test_resolve_symbol_missing_name() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_resolve_symbol",
            "--request-json", "{}",
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "error");
    assert_eq!(v["warnings"][0]["code"], "INVALID_REQUEST");
}

#[test]
fn test_semantic_query_invalid_action() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"invalid","entity":"function"}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "error");
}

#[test]
fn test_semantic_query_missing_entity() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"list"}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "error");
    assert_eq!(v["warnings"][0]["code"], "INVALID_REQUEST");
}

#[test]
fn test_describe_symbol_not_found() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_describe_symbol",
            "--request-json", r#"{"symbol_id":"nonexistent"}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["warnings"][0]["code"], "NO_MATCH");
}

#[test]
fn test_doctor() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "doctor",
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["result_kind"], "doctor");
    assert_eq!(v["ok"], true);
}

#[test]
fn test_count_action() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"count","entity":"function"}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["result_kind"], "count");
    assert_eq!(v["count"], 4);
}

#[test]
fn test_exists_action() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"exists","entity":"function"}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["result_kind"], "exists");
    assert_eq!(v["exists"], true);
}

#[test]
fn test_list_functions_legacy() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "list-functions",
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"add"));
    assert!(names.contains(&"square"));
    assert!(names.contains(&"combined"));
}

#[test]
fn test_pagination() {
    let ws = workspace_root();
    // List functions with limit=2
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"list","entity":"function","limit":2}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    assert_eq!(v["page"]["truncated"], true);
    assert_eq!(v["page"]["total_matches"], 4);
    assert!(v["page"]["next_cursor"].is_string());
}

#[test]
fn test_where_filter_name() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"list","entity":"function","where":{"name":"square"}}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "square");
}

#[test]
fn test_fields_filter() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"list","entity":"function","fields":["name","qualified_name"]}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    // Each item should only have name and qualified_name
    for item in items {
        let obj = item.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("qualified_name"));
        assert!(!obj.contains_key("symbol_id"));
        assert!(!obj.contains_key("location"));
    }
}

#[test]
fn test_request_file_not_found_exits_nonzero() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_resolve_symbol",
            "--request-file", "/nonexistent/file.json",
        ])
        .output()
        .expect("failed to execute");
    assert!(!output.status.success());
}

#[test]
fn test_classes_struct_list() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/data.cpp"),
            "cpp_semantic_query",
            "--request-json", r#"{"action":"count","entity":"struct"}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["count"], 1); // struct Point
}

#[test]
fn test_resolve_with_entity_filter() {
    let ws = workspace_root();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args(&[
            "--build-dir", &format!("{ws}/build"),
            "--file", &format!("{ws}/samples/cpp/functions.cpp"),
            "cpp_resolve_symbol",
            "--request-json", r#"{"name":"add","entity":"function"}"#,
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["ambiguous"], true);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    for item in items {
        assert_eq!(item["entity"], "function");
        assert_eq!(item["name"], "add");
    }
}
