/// Tool implementations: cpp_semantic_query.
///
/// Each tool takes an IndexData + request, and returns a JSON response as
/// `serde_json::Value`, matching the Python output format exactly.
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

use crate::clang_wrapper::norm;
use crate::index::{is_in_file, passes_scope, passes_where, IndexData, SymbolEntry};
use crate::types::{error_base, page_json};

fn parse_cursor(s: Option<&str>) -> usize {
    match s {
        None => 0,
        Some(v) => v.parse::<usize>().unwrap_or(0),
    }
}

fn page_slice(items: &[Value], limit: usize, cursor: Option<&str>) -> (Vec<Value>, Value) {
    let off = parse_cursor(cursor);
    let total = items.len();
    let end = (off + limit).min(total);
    let xs: Vec<Value> = items[off..end].to_vec();
    let nxt = off + xs.len();
    let truncated = nxt < total;
    let next_cursor = if truncated { Some(nxt.to_string()) } else { None };
    (xs, page_json(next_cursor, truncated, total))
}

// ---------------------------------------------------------------------------
// cpp_semantic_query
// ---------------------------------------------------------------------------

pub fn tool_cpp_semantic_query(idx: &IndexData, req: &serde_json::Map<String, Value>) -> Value {
    let action = req.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let entity = req.get("entity").and_then(|v| v.as_str()).unwrap_or("");

    if !["find", "list", "count", "exists"].contains(&action) {
        let mut out = error_base("INVALID_REQUEST", "action must be one of find|list|count|exists");
        let rk = if action.is_empty() { "list" } else { action };
        out.insert("result_kind".to_string(), Value::String(rk.to_string()));
        return Value::Object(out);
    }
    if entity.is_empty() {
        let mut out = error_base("INVALID_REQUEST", "entity is required");
        out.insert("result_kind".to_string(), Value::String(action.to_string()));
        return Value::Object(out);
    }

    let scope = req.get("scope").and_then(|v| v.as_object());
    let where_clause = req.get("where").and_then(|v| v.as_object());
    let include_source = req.get("include_source").and_then(|v| v.as_bool()).unwrap_or(false);
    let limit = req.get("limit").and_then(|v| v.as_i64()).unwrap_or(5000).clamp(1, 50000) as usize;
    let cursor = req.get("cursor").and_then(|v| v.as_str());
    let fields: Option<HashSet<String>> = req.get("fields").and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());

    // When no explicit scope.path / scope.file / scope.directory is given,
    // restrict results to the source file so that symbols from included
    // workspace headers don't leak into results.
    let has_path_scope = scope.map_or(false, |s| {
        s.contains_key("path") || s.contains_key("file") || s.contains_key("directory")
    });

    let matches: Vec<Value> = if entity == "file" {
        let src_norm = norm(&idx.src);
        let fname = Path::new(&idx.src).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();

        let mut file_item = serde_json::Map::new();
        file_item.insert("symbol_id".to_string(), Value::String(format!("file:{src_norm}")));
        file_item.insert("entity".to_string(), Value::String("file".to_string()));
        file_item.insert("name".to_string(), Value::String(fname.clone()));
        file_item.insert("qualified_name".to_string(), Value::String(src_norm.clone()));
        let mut loc = serde_json::Map::new();
        loc.insert("file".to_string(), Value::String(src_norm));
        file_item.insert("location".to_string(), Value::Object(loc));

        let mut items = vec![file_item.clone()];
        if let Some(wh) = where_clause {
            if let Some(Value::String(wn)) = wh.get("name") {
                if &fname != wn { items.clear(); }
            }
            if let Some(Value::String(wqn)) = wh.get("qualified_name") {
                let qn = file_item.get("qualified_name").and_then(|v| v.as_str()).unwrap_or("");
                if qn != wqn { items.clear(); }
            }
        }
        items.into_iter().map(Value::Object).collect()
    } else {
        idx.symbols.iter()
            .filter(|e| e.entity == entity)
            .filter(|e| has_path_scope || is_in_file(e, &idx.src))
            .filter(|e| passes_scope(e, scope))
            .filter(|e| passes_where(idx, e, where_clause))
            .map(|e| {
                let mut s = e.summary.clone();
                if include_source {
                    enrich_with_source(e, &mut s);
                }
                Value::Object(s)
            })
            .collect()
    };

    match action {
        "find" | "list" => {
            let (mut items, page) = page_slice(&matches, limit, cursor);
            if let Some(ref keep) = fields {
                items = items.into_iter().map(|item| {
                    if let Value::Object(m) = item {
                        Value::Object(m.into_iter().filter(|(k, _)| keep.contains(k)).collect())
                    } else { item }
                }).collect();
            }
            let mut out = serde_json::Map::new();
            out.insert("status".to_string(), Value::String("ok".to_string()));
            out.insert("result_kind".to_string(), Value::String(action.to_string()));
            out.insert("items".to_string(), Value::Array(items));
            out.insert("warnings".to_string(), Value::Array(vec![]));
            out.insert("page".to_string(), page);
            Value::Object(out)
        }
        "count" => {
            let mut out = serde_json::Map::new();
            out.insert("status".to_string(), Value::String("ok".to_string()));
            out.insert("result_kind".to_string(), Value::String("count".to_string()));
            out.insert("count".to_string(), Value::Number(matches.len().into()));
            out.insert("warnings".to_string(), Value::Array(vec![]));
            Value::Object(out)
        }
        "exists" => {
            let mut out = serde_json::Map::new();
            out.insert("status".to_string(), Value::String("ok".to_string()));
            out.insert("result_kind".to_string(), Value::String("exists".to_string()));
            out.insert("exists".to_string(), Value::Bool(!matches.is_empty()));
            out.insert("warnings".to_string(), Value::Array(vec![]));
            Value::Object(out)
        }
        _ => unreachable!(),
    }
}

/// Attach `source` and `extent` fields to a symbol summary using the entry's stored extent.
fn enrich_with_source(entry: &SymbolEntry, summary: &mut serde_json::Map<String, Value>) {
    let (start, end) = entry.extent;
    if start == 0 || end < start { return; }
    let file_path = match entry.file_norm.as_deref()
        .or_else(|| entry.summary.get("location")
            .and_then(|l| l.get("file"))
            .and_then(|f| f.as_str()))
    {
        Some(p) => p.to_string(),
        None => return,
    };
    if let Ok(content) = std::fs::read_to_string(&file_path) {
        let lines: Vec<&str> = content.lines().collect();
        let lo = (start as usize).saturating_sub(1);
        let hi = (end as usize).min(lines.len());
        let source = lines[lo..hi].join("\n");
        summary.insert("source".to_string(), Value::String(source));
        summary.insert("extent".to_string(), serde_json::json!({
            "start_line": start,
            "end_line": end
        }));
    }
}

// ---------------------------------------------------------------------------
// Legacy commands
// ---------------------------------------------------------------------------

pub fn list_functions(idx: &IndexData) -> Value {
    let items: Vec<Value> = idx.symbols.iter()
        .filter(|e| e.cursor_kind == clang_sys::CXCursor_FunctionDecl && e.is_definition && is_in_file(e, &idx.src))
        .map(|e| Value::Object(e.summary.clone()))
        .collect();

    let total = items.len();
    let mut out = serde_json::Map::new();
    out.insert("status".to_string(), Value::String("ok".to_string()));
    out.insert("result_kind".to_string(), Value::String("list".to_string()));
    out.insert("items".to_string(), Value::Array(items));
    out.insert("warnings".to_string(), Value::Array(vec![]));
    out.insert("page".to_string(), page_json(None, false, total));
    Value::Object(out)
}

pub fn describe_function(idx: &IndexData, name: &str) -> Value {
    let matches: Vec<&SymbolEntry> = idx.symbols.iter()
        .filter(|e| e.cursor_kind == clang_sys::CXCursor_FunctionDecl && e.is_definition && is_in_file(e, &idx.src) && e.name == name)
        .collect();

    if matches.is_empty() {
        let mut out = serde_json::Map::new();
        out.insert("status".to_string(), Value::String("ok".to_string()));
        out.insert("result_kind".to_string(), Value::String("describe_symbol".to_string()));
        out.insert("item".to_string(), Value::Null);
        out.insert("warnings".to_string(), Value::Array(vec![serde_json::json!({
            "code": "NO_MATCH", "message": format!("no function named {name}")
        })]));
        return Value::Object(out);
    }

    if matches.len() > 1 {
        let candidates: Vec<Value> = matches.iter().map(|e| Value::Object(e.summary.clone())).collect();
        let mut out = serde_json::Map::new();
        out.insert("status".to_string(), Value::String("ok".to_string()));
        out.insert("result_kind".to_string(), Value::String("describe_symbol".to_string()));
        out.insert("item".to_string(), Value::Null);
        out.insert("warnings".to_string(), Value::Array(vec![serde_json::json!({
            "code": "AMBIGUOUS_SYMBOL", "message": format!("multiple functions named {name}")
        })]));
        out.insert("candidates".to_string(), Value::Array(candidates));
        return Value::Object(out);
    }

    let item = matches[0].summary.clone();
    let mut out = serde_json::Map::new();
    out.insert("status".to_string(), Value::String("ok".to_string()));
    out.insert("result_kind".to_string(), Value::String("describe_symbol".to_string()));
    out.insert("item".to_string(), Value::Object(item));
    out.insert("warnings".to_string(), Value::Array(vec![]));
    Value::Object(out)
}

pub fn doctor(build_dir: Option<&str>, src: Option<&str>) -> Value {
    let mut checks: Vec<Value> = Vec::new();
    checks.push(serde_json::json!({"name": "libclang_runtime", "ok": true, "message": "libclang runtime is usable"}));

    if let Some(bd) = build_dir {
        let p = std::path::Path::new(bd);
        let db_path = p.join("compile_commands.json");
        checks.push(serde_json::json!({"name": "build_dir_exists", "ok": p.is_dir(), "message": bd}));
        checks.push(serde_json::json!({"name": "compile_commands_json", "ok": db_path.is_file(), "message": db_path.to_string_lossy()}));
    }

    if let Some(s) = src {
        let p = std::path::Path::new(s);
        checks.push(serde_json::json!({"name": "source_file_exists", "ok": p.is_file(), "message": s}));
    }

    let ok = checks.iter().all(|c| c.get("ok").and_then(|v| v.as_bool()).unwrap_or(false));
    let warnings = if ok { vec![] } else {
        vec![serde_json::json!({"code": "CHECK_FAILED", "message": "one or more doctor checks failed"})]
    };
    serde_json::json!({"status": "ok", "result_kind": "doctor", "ok": ok, "checks": checks, "warnings": warnings})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        ensure_test_build, PARSE_CPP, TEST_BUILD_DIR,
    };

    #[test]
    fn test_parse_cursor() {
        assert_eq!(parse_cursor(None), 0);
        assert_eq!(parse_cursor(Some("")), 0);
        assert_eq!(parse_cursor(Some("5")), 5);
        assert_eq!(parse_cursor(Some("abc")), 0);
    }

    #[test]
    fn test_page_slice_empty() {
        let items: Vec<Value> = vec![];
        let (result, page) = page_slice(&items, 10, None);
        assert!(result.is_empty());
        assert_eq!(page.get("truncated").unwrap(), &Value::Bool(false));
        assert_eq!(page.get("total_matches").unwrap(), &Value::Number(0.into()));
    }

    #[test]
    fn test_page_slice_pagination() {
        let items: Vec<Value> = (0..5).map(|i| Value::Number(i.into())).collect();
        let (result, page) = page_slice(&items, 2, None);
        assert_eq!(result.len(), 2);
        assert_eq!(page.get("truncated").unwrap(), &Value::Bool(true));
        assert_eq!(page.get("total_matches").unwrap(), &Value::Number(5.into()));

        let (result2, _) = page_slice(&items, 2, Some("2"));
        assert_eq!(result2.len(), 2);

        let (result3, page3) = page_slice(&items, 2, Some("4"));
        assert_eq!(result3.len(), 1);
        assert_eq!(page3.get("truncated").unwrap(), &Value::Bool(false));
    }

    #[test]
    fn test_page_slice_full_page() {
        let items: Vec<Value> = (0..3).map(|i| Value::Number(i.into())).collect();
        let (result, page) = page_slice(&items, 3, None);
        assert_eq!(result.len(), 3);
        assert_eq!(page.get("truncated").unwrap(), &Value::Bool(false));
        assert_eq!(page.get("next_cursor").unwrap(), &Value::Null);
    }

    #[test]
    fn test_page_slice_larger_limit() {
        let items: Vec<Value> = (0..3).map(|i| Value::Number(i.into())).collect();
        let (result, page) = page_slice(&items, 100, None);
        assert_eq!(result.len(), 3);
        assert_eq!(page.get("truncated").unwrap(), &Value::Bool(false));
        assert_eq!(page.get("total_matches").unwrap(), &Value::Number(3.into()));
    }

    #[test]
    fn test_doctor_no_args() {
        let result = doctor(None, None);
        assert_eq!(result.get("status").unwrap(), "ok");
    }

    #[test]
    fn test_doctor_with_valid_paths() {
        ensure_test_build();
        let result = doctor(Some(TEST_BUILD_DIR), Some(PARSE_CPP));
        assert_eq!(result["status"], "ok");
        assert_eq!(result["ok"], true);
    }

    #[test]
    fn test_doctor_with_invalid_build_dir() {
        let result = doctor(Some("/nonexistent/build"), None);
        assert_eq!(result["status"], "ok");
        assert_eq!(result["ok"], false);
    }

    fn build_functions_index() -> crate::index::IndexData {
        ensure_test_build();
        crate::index::load_index(TEST_BUILD_DIR, PARSE_CPP, None)
            .expect("failed to load index")
    }

    #[test]
    fn test_semantic_query_list() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("list".to_string()));
        req.insert("entity".to_string(), Value::String("function".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert_eq!(result["status"], "ok");
        assert!(result["items"].as_array().unwrap().len() >= 8);
    }

    #[test]
    fn test_semantic_query_count() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("count".to_string()));
        req.insert("entity".to_string(), Value::String("function".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert!(result["count"].as_i64().unwrap_or(0) >= 8);
    }

    #[test]
    fn test_semantic_query_exists_true() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("exists".to_string()));
        req.insert("entity".to_string(), Value::String("function".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert_eq!(result["exists"], true);
    }

    #[test]
    fn test_semantic_query_exists_false() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("exists".to_string()));
        req.insert("entity".to_string(), Value::String("class".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert_eq!(result["exists"], false);
    }

    #[test]
    fn test_semantic_query_invalid_action() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("bogus".to_string()));
        req.insert("entity".to_string(), Value::String("function".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert_eq!(result["status"], "error");
    }

    #[test]
    fn test_semantic_query_missing_entity() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("list".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert_eq!(result["status"], "error");
    }

    #[test]
    fn test_semantic_query_where_filter() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("list".to_string()));
        req.insert("entity".to_string(), Value::String("function".to_string()));
        req.insert("where".to_string(), serde_json::json!({"name": "LoadFile"}));
        let result = tool_cpp_semantic_query(&idx, &req);
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "LoadFile");
    }

    #[test]
    fn test_semantic_query_fields() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("list".to_string()));
        req.insert("entity".to_string(), Value::String("function".to_string()));
        req.insert("fields".to_string(), serde_json::json!(["name", "qualified_name"]));
        let result = tool_cpp_semantic_query(&idx, &req);
        for item in result["items"].as_array().unwrap() {
            let obj = item.as_object().unwrap();
            assert!(obj.contains_key("name"));
            assert!(!obj.contains_key("symbol_id"));
        }
    }

    #[test]
    fn test_semantic_query_pagination() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("list".to_string()));
        req.insert("entity".to_string(), Value::String("function".to_string()));
        req.insert("limit".to_string(), Value::Number(2.into()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert_eq!(result["items"].as_array().unwrap().len(), 2);
        assert_eq!(result["page"]["truncated"], true);
        assert!(result["page"]["total_matches"].as_i64().unwrap_or(0) >= 8);
    }

    #[test]
    fn test_semantic_query_file_entity() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("list".to_string()));
        req.insert("entity".to_string(), Value::String("file".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["entity"], "file");
    }

    #[test]
    fn test_semantic_query_calls() {
        let idx = build_functions_index();
        let mut req = serde_json::Map::new();
        req.insert("action".to_string(), Value::String("list".to_string()));
        req.insert("entity".to_string(), Value::String("call".to_string()));
        let result = tool_cpp_semantic_query(&idx, &req);
        assert!(!result["items"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_list_functions_legacy() {
        let idx = build_functions_index();
        let result = list_functions(&idx);
        assert_eq!(result["status"], "ok");
        assert!(result["items"].as_array().unwrap().len() >= 8);
    }

    #[test]
    fn test_describe_function_found() {
        let idx = build_functions_index();
        let result = describe_function(&idx, "LoadFile");
        assert_eq!(result["item"]["name"], "LoadFile");
    }

    #[test]
    fn test_describe_function_not_found() {
        let idx = build_functions_index();
        let result = describe_function(&idx, "nonexistent");
        assert_eq!(result["warnings"][0]["code"], "NO_MATCH");
    }

    #[test]
    fn test_describe_function_ambiguous() {
        let idx = build_functions_index();
        let result = describe_function(&idx, "Load");
        assert_eq!(result["warnings"][0]["code"], "AMBIGUOUS_SYMBOL");
        assert!(result["candidates"].is_array());
    }
}
