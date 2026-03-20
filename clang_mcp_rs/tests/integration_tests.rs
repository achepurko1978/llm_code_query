use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use std::process::Command as StdCommand;
use std::sync::OnceLock;

const BUILD_DIR: &str = "/workspace/samples/cpp/build-rust-tests";
const PARSE_CPP: &str = "/workspace/samples/cpp/src/parse.cpp";
const NODE_H: &str = "/workspace/samples/cpp/include/yaml-cpp/node/node.h";
const EMIT_FROM_EVENTS_H: &str = "/workspace/samples/cpp/include/yaml-cpp/emitfromevents.h";

fn ensure_fixture() {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    let result = INIT.get_or_init(|| {
        let compile_db = format!("{BUILD_DIR}/compile_commands.json");
        if Path::new(&compile_db).is_file() {
            return Ok(());
        }

        let status = StdCommand::new("cmake")
            .args([
                "-S",
                "/workspace/samples/cpp",
                "-B",
                BUILD_DIR,
                "-G",
                "Ninja",
                "-D",
                "CMAKE_CXX_COMPILER=clang++",
                "-D",
                "CMAKE_EXPORT_COMPILE_COMMANDS=ON",
            ])
            .status()
            .map_err(|e| format!("failed to run cmake configure: {e}"))?;

        if !status.success() {
            return Err("cmake configure failed for integration fixture".to_string());
        }

        if !Path::new(&compile_db).is_file() {
            return Err(format!("compile database not generated at {compile_db}"));
        }
        Ok(())
    });

    if let Err(msg) = result {
        panic!("{msg}");
    }
}

fn run_tool_json(file: &str, tool: &str, request_json: &str) -> Value {
    ensure_fixture();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args([
            "--build-dir",
            BUILD_DIR,
            "--file",
            file,
            tool,
            "--request-json",
            request_json,
        ])
        .output()
        .expect("failed to execute binary");
    assert!(
        output.status.success(),
        "binary exited with error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("invalid JSON output")
}

#[test]
fn test_doctor_ok() {
    ensure_fixture();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args([
            "--build-dir",
            BUILD_DIR,
            "--file",
            PARSE_CPP,
            "doctor",
        ])
        .output()
        .expect("failed to execute binary");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["ok"], true);
}

#[test]
fn test_semantic_list_functions_parse_contains_loadfile() {
    let v = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"list","entity":"function","fields":["name"]}"#,
    );
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    assert!(items.len() >= 8);
    assert!(items.iter().any(|i| i["name"] == "LoadFile"));
}

#[test]
fn test_semantic_count_and_exists_actions() {
    let count = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"count","entity":"call"}"#,
    );
    assert_eq!(count["status"], "ok");
    assert!(count["count"].as_i64().unwrap_or(0) > 0);

    let exists = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"exists","entity":"class"}"#,
    );
    assert_eq!(exists["status"], "ok");
    assert_eq!(exists["exists"], false);
}

#[test]
fn test_semantic_where_fields_and_pagination() {
    let where_v = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"list","entity":"function","where":{"name":"LoadFile"}}"#,
    );
    let where_items = where_v["items"].as_array().unwrap();
    assert_eq!(where_items.len(), 1);
    assert_eq!(where_items[0]["name"], "LoadFile");

    let fields_v = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"list","entity":"function","fields":["name","qualified_name"]}"#,
    );
    for item in fields_v["items"].as_array().unwrap() {
        let obj = item.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("qualified_name"));
        assert!(!obj.contains_key("symbol_id"));
    }

    let page_v = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"list","entity":"function","limit":2}"#,
    );
    assert_eq!(page_v["items"].as_array().unwrap().len(), 2);
    assert_eq!(page_v["page"]["truncated"], true);
    assert!(page_v["page"]["next_cursor"].is_string());
}

#[test]
fn test_semantic_find_action_and_source_include() {
    let find_v = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"find","entity":"function","where":{"name":"LoadFile"}}"#,
    );
    assert_eq!(find_v["status"], "ok");
    assert_eq!(find_v["result_kind"], "find");
    assert_eq!(find_v["items"].as_array().unwrap().len(), 1);

    let src_v = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"list","entity":"function","where":{"name":"LoadFile"},"include_source":true}"#,
    );
    let item = &src_v["items"].as_array().unwrap()[0];
    assert!(item["source"].as_str().unwrap_or("").contains("LoadFile"));
    assert!(item["extent"].is_object());
}

#[test]
fn test_semantic_query_call_name_filter() {
    let v = run_tool_json(
        PARSE_CPP,
        "cpp_semantic_query",
        r#"{"action":"list","entity":"call","where":{"name":"Parser"},"fields":["name"]}"#,
    );
    assert_eq!(v["status"], "ok");
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items.iter().all(|i| i["name"] == "Parser"));
}

#[test]
fn test_header_queries_node_and_emitfromevents() {
    let node_classes = run_tool_json(
        NODE_H,
        "cpp_semantic_query",
        r#"{"action":"count","entity":"class"}"#,
    );
    assert_eq!(node_classes["status"], "ok");
    assert!(node_classes["count"].as_i64().unwrap_or(0) >= 3);

    let node_methods = run_tool_json(
        NODE_H,
        "cpp_semantic_query",
        r#"{"action":"count","entity":"method"}"#,
    );
    assert!(node_methods["count"].as_i64().unwrap_or(0) >= 10);

    let overrides = run_tool_json(
        EMIT_FROM_EVENTS_H,
        "cpp_semantic_query",
        r#"{"action":"exists","entity":"method","where":{"override":true}}"#,
    );
    assert_eq!(overrides["status"], "ok");
    assert_eq!(overrides["exists"], true);
}

#[test]
fn test_legacy_list_functions_nonempty() {
    ensure_fixture();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args([
            "--build-dir",
            BUILD_DIR,
            "--file",
            PARSE_CPP,
            "list-functions",
        ])
        .output()
        .expect("failed to execute binary");
    assert!(output.status.success());
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert!(!v["items"].as_array().unwrap().is_empty());
}

#[test]
fn test_request_file_not_found_exits_nonzero() {
    ensure_fixture();
    let output = Command::cargo_bin("clang_mcp")
        .unwrap()
        .env("LIBCLANG_PATH", "/usr/lib/x86_64-linux-gnu")
        .args([
            "--build-dir",
            BUILD_DIR,
            "--file",
            PARSE_CPP,
            "cpp_semantic_query",
            "--request-file",
            "/nonexistent/file.json",
        ])
        .output()
        .expect("failed to execute");
    assert!(!output.status.success());
}
