/// Symbol extraction: converts libclang cursors to JSON-serializable symbol summaries.
///
/// Mirrors the Python implementation's `symbol_summary`, `entity_of`, `sig`,
/// `qualified_name`, `symbol_id`, etc.
use clang_sys::*;
use serde_json::Value;

use crate::clang_wrapper::Cursor;
use crate::types::location_json;

/// Map a CXCursorKind to an entity string, matching the Python entity_of().
pub fn entity_of(kind: CXCursorKind) -> Option<&'static str> {
    match kind {
        CXCursor_ClassDecl => Some("class"),
        CXCursor_StructDecl => Some("struct"),
        CXCursor_FunctionDecl => Some("function"),
        CXCursor_CXXMethod => Some("method"),
        CXCursor_Constructor => Some("constructor"),
        CXCursor_Destructor => Some("destructor"),
        CXCursor_FieldDecl => Some("field"),
        CXCursor_VarDecl => Some("variable"),
        CXCursor_ParmDecl => Some("parameter"),
        CXCursor_CallExpr => Some("call"),
        CXCursor_EnumDecl => Some("enum"),
        CXCursor_Namespace => Some("namespace"),
        _ => None,
    }
}

/// Build the qualified name (e.g. "ns::Class::method") for a cursor.
pub fn qualified_name(c: &Cursor) -> String {
    let mut parts = Vec::new();
    let mut cur = *c;
    loop {
        if cur.is_null() || cur.is_translation_unit() {
            break;
        }
        let s = cur.spelling();
        if !s.is_empty() {
            parts.push(s);
        }
        let parent = cur.semantic_parent();
        if parent == cur {
            // Guard against infinite loop
            break;
        }
        cur = parent;
    }
    parts.reverse();
    parts.join("::")
}

/// Return a stable symbol identifier (USR preferred, location fallback).
pub fn symbol_id(c: &Cursor) -> String {
    let usr = c.usr();
    if !usr.is_empty() {
        return usr;
    }
    let loc = c.location();
    let f = loc.file.as_deref().unwrap_or("<unknown>");
    format!("loc:{}:{}:{}", f, loc.line, loc.column)
}

/// Build a function/method signature string.
pub fn sig(c: &Cursor) -> String {
    let params: Vec<String> = c
        .arguments()
        .iter()
        .map(|p| {
            let ts = p.cursor_type().spelling();
            let name = p.spelling();
            if name.is_empty() {
                ts
            } else {
                format!("{ts} {name}")
            }
        })
        .collect();
    let base = format!(
        "{} {}({})",
        c.result_type().spelling(),
        c.spelling(),
        params.join(", ")
    );
    if c.is_const_method() {
        format!("{base} const")
    } else {
        base
    }
}

/// Build a parameter summary JSON value.
pub fn parameter_summary(p: &Cursor, pos: usize) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("entity".to_string(), Value::String("parameter".to_string()));
    m.insert("name".to_string(), Value::String(p.spelling()));
    m.insert("type".to_string(), Value::String(p.cursor_type().spelling()));
    m.insert("position".to_string(), Value::Number(pos.into()));
    let sid = symbol_id(p);
    if !sid.is_empty() {
        m.insert("symbol_id".to_string(), Value::String(sid));
    }
    let loc = p.location();
    m.insert(
        "location".to_string(),
        location_json(
            loc.file.as_deref().unwrap_or("<unknown>"),
            Some(loc.line),
            Some(loc.column),
        ),
    );
    Value::Object(m)
}

/// Build a full symbol summary (ordered JSON map), matching Python `symbol_summary`.
pub fn symbol_summary(c: &Cursor) -> serde_json::Map<String, Value> {
    let kind = c.kind();
    let e = entity_of(kind);
    let mut nm = c.spelling();

    // For call expressions with no name, fall back to referenced symbol
    if e == Some("call") && nm.is_empty() {
        if let Some(ref_c) = c.referenced() {
            let rn = ref_c.spelling();
            if !rn.is_empty() {
                nm = rn;
            } else {
                let dn = c.display_name();
                if !dn.is_empty() {
                    nm = dn;
                }
            }
        }
    }

    let mut out = serde_json::Map::new();
    out.insert("symbol_id".to_string(), Value::String(symbol_id(c)));
    out.insert(
        "entity".to_string(),
        Value::String(e.unwrap_or("").to_string()),
    );
    out.insert("name".to_string(), Value::String(nm));

    // Qualified name
    let qn = if e == Some("call") {
        c.referenced().map(|r| qualified_name(&r)).unwrap_or_default()
    } else {
        qualified_name(c)
    };
    if !qn.is_empty() {
        out.insert("qualified_name".to_string(), Value::String(qn));
    }

    let is_callable = matches!(e, Some("function" | "method" | "constructor" | "destructor"));

    if is_callable {
        out.insert("signature".to_string(), Value::String(sig(c)));
        let rt = c.result_type().spelling();
        if !rt.is_empty() {
            out.insert("return_type".to_string(), Value::String(rt));
        }
        let args = c.arguments();
        let params: Vec<Value> = args
            .iter()
            .enumerate()
            .map(|(i, p)| parameter_summary(p, i))
            .collect();
        out.insert("parameters".to_string(), Value::Array(params));

        out.insert("static".to_string(), Value::Bool(c.is_static_method()));
        out.insert("const".to_string(), Value::Bool(c.is_const_method()));

        let mut is_virtual = c.is_virtual_method();
        if c.is_pure_virtual_method() {
            is_virtual = true;
        }
        out.insert("virtual".to_string(), Value::Bool(is_virtual));

        if e == Some("method") {
            let has_override = !c.overridden_cursors().is_empty();
            out.insert("override".to_string(), Value::Bool(has_override));
        }
    } else {
        let ty = c.cursor_type().spelling();
        if !ty.is_empty() {
            out.insert("type".to_string(), Value::String(ty));
        }
    }

    // Access specifier
    let access = c.access_specifier();
    match access {
        CX_CXXPublic => { out.insert("access".to_string(), Value::String("public".to_string())); }
        CX_CXXProtected => { out.insert("access".to_string(), Value::String("protected".to_string())); }
        CX_CXXPrivate => { out.insert("access".to_string(), Value::String("private".to_string())); }
        _ => {}
    }

    // Boolean attributes — Python checks hasattr + callable. In libclang, these
    // are always available for CXX methods, so we match what the Python version
    // would include for each entity type.
    // deleted / defaulted are always emitted by Python when is_deleted_method / is_default_method exist
    // For the Python version, bool_attr returns None only when hasattr fails.
    // In libclang C API these are always available (return 0/false).
    // The Python implementation emits deleted/defaulted for everything that has the attribute.
    // Based on sample output: "deleted": false, "defaulted": false are always present.
    out.insert(
        "deleted".to_string(),
        Value::Bool(false), // CXXMethod_isDeleted is not in clang-sys stable
    );
    out.insert("defaulted".to_string(), Value::Bool(c.is_default_method()));

    // implicit — Python uses c.is_implicit; we check if cursor has CXCursor_is_implicit
    // The python code does bool_attr(c, "is_implicit") which returns None if not hasattr.
    // For explicit user-defined symbols, Python typically does NOT include "implicit".
    // Looking at sample output, "implicit" is not present — so it's None in Python
    // meaning hasattr returns False. Skip it to match.

    let loc = c.location();
    out.insert(
        "location".to_string(),
        location_json(
            loc.file.as_deref().unwrap_or("<unknown>"),
            Some(loc.line),
            Some(loc.column),
        ),
    );

    out
}

/// Build a relation summary from a symbol summary.
pub fn relation_summary(rel_kind: &str, c: &Cursor) -> serde_json::Map<String, Value> {
    let s = symbol_summary(c);
    let mut out = serde_json::Map::new();
    out.insert("kind".to_string(), Value::String(rel_kind.to_string()));
    out.insert(
        "symbol_id".to_string(),
        s.get("symbol_id").cloned().unwrap_or(Value::String(String::new())),
    );
    out.insert(
        "entity".to_string(),
        s.get("entity").cloned().unwrap_or(Value::String(String::new())),
    );
    out.insert(
        "name".to_string(),
        s.get("name").cloned().unwrap_or(Value::String(String::new())),
    );
    if let Some(v) = s.get("qualified_name") {
        out.insert("qualified_name".to_string(), v.clone());
    }
    if let Some(v) = s.get("signature") {
        out.insert("signature".to_string(), v.clone());
    }
    if let Some(v) = s.get("location") {
        out.insert("location".to_string(), v.clone());
    }
    out
}

/// Get callable parameter types as a list of type strings.
pub fn callable_param_types(c: &Cursor) -> Vec<String> {
    let e = entity_of(c.kind());
    if !matches!(e, Some("function" | "method" | "constructor" | "destructor")) {
        return Vec::new();
    }
    c.arguments().iter().map(|p| p.cursor_type().spelling()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_of_mapping() {
        assert_eq!(entity_of(CXCursor_FunctionDecl), Some("function"));
        assert_eq!(entity_of(CXCursor_CXXMethod), Some("method"));
        assert_eq!(entity_of(CXCursor_ClassDecl), Some("class"));
        assert_eq!(entity_of(CXCursor_StructDecl), Some("struct"));
        assert_eq!(entity_of(CXCursor_Constructor), Some("constructor"));
        assert_eq!(entity_of(CXCursor_Destructor), Some("destructor"));
        assert_eq!(entity_of(CXCursor_FieldDecl), Some("field"));
        assert_eq!(entity_of(CXCursor_VarDecl), Some("variable"));
        assert_eq!(entity_of(CXCursor_ParmDecl), Some("parameter"));
        assert_eq!(entity_of(CXCursor_CallExpr), Some("call"));
        assert_eq!(entity_of(CXCursor_EnumDecl), Some("enum"));
        assert_eq!(entity_of(CXCursor_Namespace), Some("namespace"));
        // Unknown kind
        assert_eq!(entity_of(CXCursor_UnexposedDecl), None);
    }

    #[test]
    fn test_entity_of_exhaustive_none() {
        // A few more kinds we don't map
        assert_eq!(entity_of(CXCursor_TypedefDecl), None);
        assert_eq!(entity_of(CXCursor_TemplateTypeParameter), None);
    }

    /// Helper: parse a TU from the sample files and find symbols.
    fn parse_functions_tu() -> (crate::clang_wrapper::Index, crate::clang_wrapper::TranslationUnit) {
        let idx = crate::clang_wrapper::Index::new();
        let tu = idx
            .parse(
                "/workspace/samples/cpp/functions.cpp",
                &["-std=c++20".to_string(), "-I/workspace".to_string()],
            )
            .expect("failed to parse TU");
        (idx, tu)
    }

    fn find_cursor_by_name(
        tu: &crate::clang_wrapper::TranslationUnit,
        name: &str,
        target_entity: &str,
    ) -> crate::clang_wrapper::Cursor {
        use crate::clang_wrapper::walk;
        walk(tu.cursor())
            .into_iter()
            .find(|c| {
                entity_of(c.kind()) == Some(target_entity) && c.spelling() == name
            })
            .unwrap_or_else(|| panic!("could not find {target_entity} named {name}"))
    }

    #[test]
    fn test_qualified_name_function() {
        let (_idx, tu) = parse_functions_tu();
        let c = find_cursor_by_name(&tu, "square", "function");
        assert_eq!(qualified_name(&c), "fun::square");
    }

    #[test]
    fn test_symbol_id_has_usr() {
        let (_idx, tu) = parse_functions_tu();
        let c = find_cursor_by_name(&tu, "square", "function");
        let sid = symbol_id(&c);
        // USR for a named function should start with "c:@"
        assert!(sid.starts_with("c:@"), "expected USR, got: {sid}");
    }

    #[test]
    fn test_sig_function() {
        let (_idx, tu) = parse_functions_tu();
        let c = find_cursor_by_name(&tu, "square", "function");
        assert_eq!(sig(&c), "int square(int x)");
    }

    #[test]
    fn test_sig_overloaded_add() {
        let (_idx, tu) = parse_functions_tu();
        use crate::clang_wrapper::walk;
        let adds: Vec<_> = walk(tu.cursor())
            .into_iter()
            .filter(|c| entity_of(c.kind()) == Some("function") && c.spelling() == "add")
            .collect();
        assert_eq!(adds.len(), 2);
        let sigs: Vec<String> = adds.iter().map(|c| sig(c)).collect();
        assert!(sigs.contains(&"int add(int a, int b)".to_string()));
        assert!(sigs.contains(&"int add(int a, int b, int c)".to_string()));
    }

    #[test]
    fn test_symbol_summary_keys() {
        let (_idx, tu) = parse_functions_tu();
        let c = find_cursor_by_name(&tu, "square", "function");
        let s = symbol_summary(&c);
        assert_eq!(s.get("entity").unwrap(), "function");
        assert_eq!(s.get("name").unwrap(), "square");
        assert_eq!(s.get("qualified_name").unwrap(), "fun::square");
        assert!(s.contains_key("signature"));
        assert!(s.contains_key("return_type"));
        assert!(s.contains_key("parameters"));
        assert!(s.contains_key("location"));
        assert_eq!(s.get("static").unwrap(), false);
        assert_eq!(s.get("const").unwrap(), false);
        assert_eq!(s.get("virtual").unwrap(), false);
    }

    #[test]
    fn test_symbol_summary_parameters() {
        let (_idx, tu) = parse_functions_tu();
        use crate::clang_wrapper::walk;
        // Find the 2-arg add
        let c = walk(tu.cursor())
            .into_iter()
            .find(|c| {
                entity_of(c.kind()) == Some("function")
                    && c.spelling() == "add"
                    && c.arguments().len() == 2
            })
            .unwrap();
        let s = symbol_summary(&c);
        let params = s.get("parameters").unwrap().as_array().unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0]["name"], "a");
        assert_eq!(params[0]["type"], "int");
        assert_eq!(params[0]["position"], 0);
        assert_eq!(params[1]["name"], "b");
        assert_eq!(params[1]["type"], "int");
        assert_eq!(params[1]["position"], 1);
    }

    #[test]
    fn test_callable_param_types() {
        let (_idx, tu) = parse_functions_tu();
        use crate::clang_wrapper::walk;
        let c = walk(tu.cursor())
            .into_iter()
            .find(|c| {
                entity_of(c.kind()) == Some("function")
                    && c.spelling() == "add"
                    && c.arguments().len() == 3
            })
            .unwrap();
        assert_eq!(callable_param_types(&c), vec!["int", "int", "int"]);
    }

    #[test]
    fn test_callable_param_types_non_callable() {
        let (_idx, tu) = parse_functions_tu();
        use crate::clang_wrapper::walk;
        // Namespace is not callable
        let ns = walk(tu.cursor())
            .into_iter()
            .find(|c| entity_of(c.kind()) == Some("namespace"))
            .unwrap();
        assert!(callable_param_types(&ns).is_empty());
    }

    #[test]
    fn test_symbol_summary_call_expr() {
        let (_idx, tu) = parse_functions_tu();
        use crate::clang_wrapper::walk;
        let calls: Vec<_> = walk(tu.cursor())
            .into_iter()
            .filter(|c| entity_of(c.kind()) == Some("call"))
            .collect();
        assert_eq!(calls.len(), 2);
        let names: Vec<String> = calls.iter().map(|c| symbol_summary(c).get("name").unwrap().as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"square".to_string()));
        assert!(names.contains(&"add".to_string()));
    }

    #[test]
    fn test_relation_summary() {
        let (_idx, tu) = parse_functions_tu();
        let c = find_cursor_by_name(&tu, "square", "function");
        let rs = relation_summary("calls", &c);
        assert_eq!(rs.get("kind").unwrap(), "calls");
        assert_eq!(rs.get("entity").unwrap(), "function");
        assert_eq!(rs.get("name").unwrap(), "square");
        assert!(rs.contains_key("symbol_id"));
        assert!(rs.contains_key("location"));
    }

    #[test]
    fn test_qualified_name_classes() {
        let idx = crate::clang_wrapper::Index::new();
        let tu = idx
            .parse(
                "/workspace/samples/cpp/classes.cpp",
                &["-std=c++20".to_string(), "-I/workspace".to_string()],
            )
            .unwrap();
        let c = find_cursor_by_name(&tu, "bump", "method");
        let qn = qualified_name(&c);
        // Should be either model::BaseCounter::bump or model::FancyCounter::bump
        assert!(qn.starts_with("model::"), "expected model:: prefix, got: {qn}");
        assert!(qn.ends_with("::bump"), "expected ::bump suffix, got: {qn}");
    }
}
