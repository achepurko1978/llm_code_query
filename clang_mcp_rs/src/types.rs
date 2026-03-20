/// JSON-serializable types that match the output schema of clang_mcp.py.
///
/// These types use `serde_json::Value` for dynamic fields and `#[serde(skip_serializing_if)]`
/// to match the Python implementation's conditional field inclusion.
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub next_cursor: Option<String>,
    pub truncated: bool,
    pub total_matches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSummary {
    pub entity: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub position: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// A symbol summary using ordered JSON map for field ordering fidelity.
#[allow(dead_code)]
pub type SymbolSummary = serde_json::Map<String, Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationSummary {
    pub kind: String,
    pub symbol_id: String,
    pub entity: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

// ---------------------------------------------------------------------------
// Doctor types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

/// Helper to build a Location JSON value.
pub fn location_json(file: &str, line: Option<u32>, column: Option<u32>) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("file".to_string(), Value::String(file.to_string()));
    if let Some(l) = line {
        if l > 0 {
            m.insert("line".to_string(), Value::Number(l.into()));
        }
    }
    if let Some(c) = column {
        if c > 0 {
            m.insert("column".to_string(), Value::Number(c.into()));
        }
    }
    Value::Object(m)
}

/// Helpers for building response JSON objects with consistent field ordering.
#[allow(dead_code)]
pub fn ok_base() -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("status".to_string(), Value::String("ok".to_string()));
    m.insert("warnings".to_string(), Value::Array(vec![]));
    m
}

pub fn error_base(code: &str, message: &str) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("status".to_string(), Value::String("error".to_string()));
    let w = serde_json::json!({"code": code, "message": message});
    m.insert("warnings".to_string(), Value::Array(vec![w]));
    m
}

pub fn page_json(next_cursor: Option<String>, truncated: bool, total_matches: usize) -> Value {
    let mut m = serde_json::Map::new();
    m.insert(
        "next_cursor".to_string(),
        match next_cursor {
            Some(c) => Value::String(c),
            None => Value::Null,
        },
    );
    m.insert("truncated".to_string(), Value::Bool(truncated));
    m.insert(
        "total_matches".to_string(),
        Value::Number(total_matches.into()),
    );
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_json_full() {
        let loc = location_json("/foo/bar.cpp", Some(10), Some(5));
        let obj = loc.as_object().unwrap();
        assert_eq!(obj["file"], "/foo/bar.cpp");
        assert_eq!(obj["line"], 10);
        assert_eq!(obj["column"], 5);
    }

    #[test]
    fn test_location_json_no_line_col() {
        let loc = location_json("/foo/bar.cpp", None, None);
        let obj = loc.as_object().unwrap();
        assert_eq!(obj["file"], "/foo/bar.cpp");
        assert!(!obj.contains_key("line"));
        assert!(!obj.contains_key("column"));
    }

    #[test]
    fn test_location_json_zero_line() {
        let loc = location_json("/foo/bar.cpp", Some(0), Some(0));
        let obj = loc.as_object().unwrap();
        assert!(!obj.contains_key("line"));
        assert!(!obj.contains_key("column"));
    }

    #[test]
    fn test_error_base() {
        let err = error_base("TEST_CODE", "test message");
        assert_eq!(err["status"], "error");
        let warnings = err["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0]["code"], "TEST_CODE");
        assert_eq!(warnings[0]["message"], "test message");
    }

    #[test]
    fn test_ok_base() {
        let ok = ok_base();
        assert_eq!(ok["status"], "ok");
        assert_eq!(ok["warnings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_page_json_no_cursor() {
        let page = page_json(None, false, 5);
        let obj = page.as_object().unwrap();
        assert_eq!(obj["next_cursor"], Value::Null);
        assert_eq!(obj["truncated"], false);
        assert_eq!(obj["total_matches"], 5);
    }

    #[test]
    fn test_page_json_with_cursor() {
        let page = page_json(Some("10".to_string()), true, 25);
        let obj = page.as_object().unwrap();
        assert_eq!(obj["next_cursor"], "10");
        assert_eq!(obj["truncated"], true);
        assert_eq!(obj["total_matches"], 25);
    }
}
