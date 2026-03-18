/// Semantic index: collects all symbols and their relationships from a translation unit.
///
/// Stores pre-serialized symbol summaries (not raw cursors) so the
/// `Index` and `TranslationUnit` can be dropped after indexing.
use std::collections::{HashMap, HashSet};

use clang_sys::*;
use serde_json::Value;

use crate::clang_wrapper::{Cursor, Index, TranslationUnit, norm, walk};
use crate::compile_db;
use crate::symbols::{
    callable_param_types, entity_of, relation_summary, symbol_id, symbol_summary,
};

/// Segments in file paths that mark external (system/vendor) code.
const EXTERNAL_SEGMENTS: &[&str] = &[
    "/.conan2/",
    "/_deps/",
    "/usr/include",
    "/usr/lib",
    "/usr/local/include",
];

fn is_workspace_file(file_path: &str, workspace_root: &str) -> bool {
    let n = norm(file_path);
    if !n.starts_with(workspace_root) {
        return false;
    }
    for seg in EXTERNAL_SEGMENTS {
        if n.contains(seg) {
            return false;
        }
    }
    true
}

/// Pre-extracted data for a single symbol, stored as owned data.
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub summary: serde_json::Map<String, Value>,
    pub symbol_id: String,
    pub entity: String,
    pub name: String,
    pub cursor_kind: CXCursorKind,
    pub is_definition: bool,
    pub file_norm: Option<String>,
    pub param_types: Vec<String>,
}

/// All indexed data for a single translation unit.
pub struct IndexData {
    pub src: String,
    pub symbols: Vec<SymbolEntry>,
    pub by_id: HashMap<String, usize>,
    pub calls_by_caller: HashMap<String, Vec<String>>,
    pub called_by_target: HashMap<String, Vec<String>>,
    pub bases_by_derived: HashMap<String, Vec<String>>,
    pub overrides_by_method: HashMap<String, Vec<String>>,
    pub contains_by_parent: HashMap<String, Vec<String>>,
    pub relation_summaries: HashMap<String, serde_json::Map<String, Value>>,
}

/// Build a semantic index from scratch: parse the file and extract everything.
pub fn load_index(build_dir: &str, src: &str, workspace_root: Option<&str>) -> anyhow::Result<IndexData> {
    let is_header = compile_db::is_header_file(src);
    let mut args = if is_header {
        // Header files are not direct compilation units; borrow flags from
        // a related source file, falling back to the first DB entry.
        match compile_db::compile_args(build_dir, src) {
            Ok(a) => a,
            Err(_) => compile_db::header_compile_args(build_dir, src)?,
        }
    } else {
        compile_db::compile_args(build_dir, src)?
    };
    if is_header {
        // Tell clang the file is C++ even though it has a .h extension.
        args.insert(0, "c++-header".to_string());
        args.insert(0, "-x".to_string());
    }
    let clang_idx = Index::new();
    let tu = clang_idx.parse(src, &args)?;
    Ok(build_index(&tu, src, workspace_root))
}

fn build_index(tu: &TranslationUnit, src: &str, workspace_root: Option<&str>) -> IndexData {
    let src_norm = norm(src);
    let ws_root = workspace_root.map(|w| norm(w));

    let in_scope = |c: &Cursor| -> bool {
        let loc = c.location();
        match &loc.file {
            None => false,
            Some(f) => {
                if let Some(ref root) = ws_root {
                    is_workspace_file(f, root)
                } else {
                    norm(f) == src_norm
                }
            }
        }
    };

    // Phase 1: collect in-scope cursors and compute summaries
    let mut cursors: Vec<Cursor> = Vec::new();
    let mut entries: Vec<SymbolEntry> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();

    for c in walk(tu.cursor()) {
        if c.is_translation_unit() { continue; }
        let e = match entity_of(c.kind()) { Some(e) => e, None => continue };
        if !in_scope(&c) { continue; }
        let sid = symbol_id(&c);
        let summary = symbol_summary(&c);
        let file_norm = c.location().file.as_ref().map(|f| norm(f));

        let entry = SymbolEntry {
            summary,
            symbol_id: sid.clone(),
            entity: e.to_string(),
            name: c.spelling(),
            cursor_kind: c.kind(),
            is_definition: c.is_definition(),
            file_norm,
            param_types: callable_param_types(&c),
        };
        let idx = entries.len();
        entries.push(entry);
        cursors.push(c);
        by_id.insert(sid, idx);
    }

    // Phase 2: build relationships
    let mut calls_by_caller: HashMap<String, Vec<String>> = HashMap::new();
    let mut called_by_target: HashMap<String, Vec<String>> = HashMap::new();
    let mut bases_by_derived: HashMap<String, Vec<String>> = HashMap::new();
    let mut overrides_by_method: HashMap<String, Vec<String>> = HashMap::new();
    let mut contains_by_parent: HashMap<String, Vec<String>> = HashMap::new();

    for (i, c) in cursors.iter().enumerate() {
        let sid = &entries[i].symbol_id;
        let e = entries[i].entity.as_str();

        let parent = c.semantic_parent();
        if !parent.is_null() {
            if let Some(pe) = entity_of(parent.kind()) {
                if matches!(pe, "class" | "struct" | "namespace") && in_scope(&parent) {
                    let pid = symbol_id(&parent);
                    contains_by_parent.entry(pid).or_default().push(sid.clone());
                }
            }
        }

        if matches!(e, "class" | "struct") {
            for ch in c.children() {
                if ch.kind() == CXCursor_CXXBaseSpecifier {
                    if let Some(base) = ch.referenced() {
                        if let Some(be) = entity_of(base.kind()) {
                            if matches!(be, "class" | "struct") {
                                bases_by_derived.entry(sid.clone()).or_default().push(symbol_id(&base));
                            }
                        }
                    }
                }
            }
        }

        if matches!(e, "method" | "constructor" | "destructor") {
            for ov in c.overridden_cursors() {
                overrides_by_method.entry(sid.clone()).or_default().push(symbol_id(&ov));
            }
        }

        if matches!(e, "function" | "method" | "constructor" | "destructor") {
            let mut seen = HashSet::new();
            for ch in walk(*c) {
                if ch.kind() != CXCursor_CallExpr { continue; }
                let tgt = match ch.referenced() { Some(r) => r, None => continue };
                match entity_of(tgt.kind()) {
                    Some("function" | "method" | "constructor" | "destructor") => {}
                    _ => continue,
                }
                let tid = symbol_id(&tgt);
                if !seen.insert(tid.clone()) { continue; }
                calls_by_caller.entry(sid.clone()).or_default().push(tid.clone());
                called_by_target.entry(tid).or_default().push(sid.clone());
            }
        }
    }

    // Phase 3: pre-compute relation summaries
    let mut rel_summaries: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();

    let mut all_ref_ids: HashSet<String> = HashSet::new();
    for ids in calls_by_caller.values().chain(called_by_target.values())
        .chain(bases_by_derived.values()).chain(overrides_by_method.values())
        .chain(contains_by_parent.values())
    {
        for id in ids { all_ref_ids.insert(id.clone()); }
    }

    for id in &all_ref_ids {
        if let Some(&idx) = by_id.get(id) {
            let s = &entries[idx].summary;
            let mut rs = serde_json::Map::new();
            for key in &["symbol_id", "entity", "name", "qualified_name", "signature", "location"] {
                if let Some(v) = s.get(*key) { rs.insert(key.to_string(), v.clone()); }
            }
            rel_summaries.insert(id.clone(), rs);
        }
    }

    // Search full AST for externally referenced symbols
    for c in walk(tu.cursor()) {
        let sid = symbol_id(&c);
        if all_ref_ids.contains(&sid) && !rel_summaries.contains_key(&sid) {
            let rs = relation_summary("", &c);
            rel_summaries.insert(sid, rs);
        }
    }

    IndexData {
        src: src.to_string(),
        symbols: entries,
        by_id,
        calls_by_caller,
        called_by_target,
        bases_by_derived,
        overrides_by_method,
        contains_by_parent,
        relation_summaries: rel_summaries,
    }
}

/// Check if a symbol is in the given source file.
pub fn is_in_file(entry: &SymbolEntry, src: &str) -> bool {
    entry.file_norm.as_ref().map_or(false, |f| f == &norm(src))
}

/// Check if a symbol passes the scope filter.
pub fn passes_scope(entry: &SymbolEntry, scope: Option<&serde_json::Map<String, Value>>) -> bool {
    let scope = match scope { Some(s) => s, None => return true };

    if let Some(Value::String(file)) = scope.get("file") {
        match &entry.file_norm {
            Some(f) if *f == norm(file) => {}
            _ => return false,
        }
    }

    // inside_function, inside_class, in_namespace require ancestor chains
    // not stored in this simplified version. Returning true for now.
    true
}

/// Check if a symbol passes the where filter.
pub fn passes_where(
    idx: &IndexData,
    entry: &SymbolEntry,
    where_clause: Option<&serde_json::Map<String, Value>>,
) -> bool {
    let wh = match where_clause { Some(w) => w, None => return true };
    let s = &entry.summary;

    for key in &["name", "qualified_name", "return_type", "type", "access"] {
        if let Some(want) = wh.get(*key) {
            if s.get(*key) != Some(want) { return false; }
        }
    }

    for key in &["static", "const", "virtual", "override", "deleted", "defaulted", "implicit"] {
        if let Some(want) = wh.get(*key) {
            if s.get(*key) != Some(want) { return false; }
        }
    }

    if let Some(Value::Array(want_types)) = wh.get("param_types") {
        let want: Vec<String> = want_types.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();
        if entry.param_types != want { return false; }
    }

    if let Some(Value::Object(rel)) = wh.get("relations") {
        if !relation_match(idx, &entry.symbol_id, rel) { return false; }
    }

    if let Some(Value::Array(any_of)) = wh.get("any_of") {
        let ok = any_of.iter().any(|cond| {
            match cond { Value::Object(m) => passes_where(idx, entry, Some(m)), _ => true }
        });
        if !ok { return false; }
    }

    true
}

fn relation_match(idx: &IndexData, sid: &str, where_rel: &serde_json::Map<String, Value>) -> bool {
    for (k, v) in where_rel {
        let want = match v.as_str() { Some(s) => s, None => continue };
        let vals = match k.as_str() {
            "derives_from" => idx.bases_by_derived.get(sid),
            "overrides" => idx.overrides_by_method.get(sid),
            "calls" => idx.calls_by_caller.get(sid),
            "called_by" => idx.called_by_target.get(sid),
            _ => continue,
        };
        let vals = match vals { Some(v) => v, None => return false };
        if vals.contains(&want.to_string()) { continue; }
        let names: HashSet<String> = vals.iter()
            .filter_map(|rid| idx.by_id.get(rid).and_then(|&i|
                idx.symbols[i].summary.get("qualified_name").and_then(|v| v.as_str()).map(String::from)))
            .collect();
        if !names.contains(want) { return false; }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_segments() {
        assert!(!is_workspace_file("/usr/include/stdio.h", "/workspace"));
        assert!(!is_workspace_file("/workspace/_deps/foo/bar.h", "/workspace"));
        assert!(is_workspace_file("/workspace/src/main.cpp", "/workspace"));
    }

    #[test]
    fn test_is_workspace_file_conan() {
        assert!(!is_workspace_file("/workspace/.conan2/pkg/inc.h", "/workspace"));
    }

    #[test]
    fn test_is_workspace_file_outside_root() {
        assert!(!is_workspace_file("/other/project/main.cpp", "/workspace"));
    }

    fn build_functions_index() -> IndexData {
        load_index("/workspace/build", "/workspace/samples/cpp/functions.cpp", None)
            .expect("failed to load index")
    }

    fn build_classes_index() -> IndexData {
        load_index("/workspace/build", "/workspace/samples/cpp/classes.cpp", None)
            .expect("failed to load index")
    }

    fn build_data_index() -> IndexData {
        load_index("/workspace/build", "/workspace/samples/cpp/data.cpp", None)
            .expect("failed to load index")
    }

    #[test]
    fn test_build_index_functions() {
        let idx = build_functions_index();
        assert!(!idx.symbols.is_empty());
        let funcs: Vec<_> = idx.symbols.iter()
            .filter(|e| e.entity == "function")
            .collect();
        assert_eq!(funcs.len(), 4);
    }

    #[test]
    fn test_build_index_calls() {
        let idx = build_functions_index();
        let calls: Vec<_> = idx.symbols.iter()
            .filter(|e| e.entity == "call")
            .collect();
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn test_build_index_by_id() {
        let idx = build_functions_index();
        for (i, e) in idx.symbols.iter().enumerate() {
            assert_eq!(idx.by_id.get(&e.symbol_id), Some(&i), "missing by_id for: {}", e.symbol_id);
        }
    }

    #[test]
    fn test_call_relationships() {
        let idx = build_functions_index();
        let combined_id = idx.symbols.iter()
            .find(|e| e.name == "combined" && e.entity == "function")
            .map(|e| e.symbol_id.clone())
            .expect("combined not found");
        let calls = idx.calls_by_caller.get(&combined_id).expect("no calls for combined");
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn test_base_class_relationships() {
        let idx = build_classes_index();
        let fancy_id = idx.symbols.iter()
            .find(|e| e.name == "FancyCounter" && e.entity == "class")
            .map(|e| e.symbol_id.clone())
            .expect("FancyCounter not found");
        let bases = idx.bases_by_derived.get(&fancy_id).expect("no bases for FancyCounter");
        assert_eq!(bases.len(), 1);
    }

    #[test]
    fn test_override_relationships() {
        let idx = build_classes_index();
        let methods: Vec<_> = idx.symbols.iter()
            .filter(|e| e.name == "bump" && e.entity == "method")
            .collect();
        let has_override = methods.iter().any(|e| {
            idx.overrides_by_method.contains_key(&e.symbol_id)
        });
        assert!(has_override, "expected at least one bump to have overrides");
    }

    #[test]
    fn test_contains_relationships() {
        let idx = build_functions_index();
        let ns_id = idx.symbols.iter()
            .find(|e| e.name == "fun" && e.entity == "namespace")
            .map(|e| e.symbol_id.clone())
            .expect("namespace 'fun' not found");
        let children = idx.contains_by_parent.get(&ns_id).expect("no children for fun");
        assert!(children.len() >= 4);
    }

    #[test]
    fn test_is_in_file() {
        let idx = build_functions_index();
        for e in &idx.symbols {
            assert!(is_in_file(e, "/workspace/samples/cpp/functions.cpp"));
        }
    }

    #[test]
    fn test_passes_scope_none_always_passes() {
        let idx = build_functions_index();
        for e in &idx.symbols {
            assert!(passes_scope(e, None));
        }
    }

    #[test]
    fn test_passes_scope_file_filter() {
        let idx = build_functions_index();
        let mut scope = serde_json::Map::new();
        scope.insert("file".to_string(), Value::String("/workspace/samples/cpp/functions.cpp".to_string()));
        for e in &idx.symbols {
            assert!(passes_scope(e, Some(&scope)));
        }
        scope.insert("file".to_string(), Value::String("/workspace/samples/cpp/classes.cpp".to_string()));
        for e in &idx.symbols {
            assert!(!passes_scope(e, Some(&scope)));
        }
    }

    #[test]
    fn test_passes_where_name_filter() {
        let idx = build_functions_index();
        let mut wh = serde_json::Map::new();
        wh.insert("name".to_string(), Value::String("square".to_string()));
        let matching: Vec<_> = idx.symbols.iter()
            .filter(|e| e.entity == "function")
            .filter(|e| passes_where(&idx, e, Some(&wh)))
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name, "square");
    }

    #[test]
    fn test_passes_where_boolean_filter() {
        let idx = build_classes_index();
        let mut wh = serde_json::Map::new();
        wh.insert("virtual".to_string(), Value::Bool(true));
        let matching: Vec<_> = idx.symbols.iter()
            .filter(|e| e.entity == "method")
            .filter(|e| passes_where(&idx, e, Some(&wh)))
            .collect();
        assert!(matching.len() >= 1);
    }

    #[test]
    fn test_data_index_struct() {
        let idx = build_data_index();
        let structs: Vec<_> = idx.symbols.iter()
            .filter(|e| e.entity == "struct")
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "Point");
    }

    #[test]
    fn test_passes_where_none_always_passes() {
        let idx = build_functions_index();
        for e in &idx.symbols {
            assert!(passes_where(&idx, e, None));
        }
    }

    #[test]
    fn test_relation_summaries_populated() {
        let idx = build_functions_index();
        // At least some relation summaries should exist for call targets
        assert!(!idx.relation_summaries.is_empty());
    }

    #[test]
    fn test_load_index_header_file() {
        let idx = load_index("/workspace/build", "/workspace/samples/cpp/shapes.h", None)
            .expect("load_index failed for header");
        let classes: Vec<_> = idx.symbols.iter().filter(|e| e.entity == "class").collect();
        assert_eq!(classes.len(), 3);
        let names: Vec<&str> = classes.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Shape"));
        assert!(names.contains(&"Circle"));
        assert!(names.contains(&"Rectangle"));
    }

    #[test]
    fn test_load_index_header_methods() {
        let idx = load_index("/workspace/build", "/workspace/samples/cpp/shapes.h", None)
            .expect("load_index failed for header");
        let methods: Vec<_> = idx.symbols.iter().filter(|e| e.entity == "method").collect();
        // Shape has 2 pure virtual methods, Circle has 3 (area, perimeter, radius),
        // Rectangle has 4 (area, perimeter, width, height)
        assert!(methods.len() >= 9, "expected >= 9 methods, got {}", methods.len());
    }

    #[test]
    fn test_load_index_header_with_templates() {
        let idx = load_index("/workspace/build", "/workspace/samples/cpp/utils.h", None)
            .expect("load_index failed for utils.h");
        let funcs: Vec<_> = idx.symbols.iter().filter(|e| e.entity == "function").collect();
        let names: Vec<&str> = funcs.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"split"));
        assert!(names.contains(&"join"));
        assert!(names.contains(&"count_char"));
    }

    #[test]
    fn test_load_index_header_structs() {
        let idx = load_index("/workspace/build", "/workspace/samples/cpp/utils.h", None)
            .expect("load_index failed for utils.h");
        let structs: Vec<_> = idx.symbols.iter().filter(|e| e.entity == "struct").collect();
        let names: Vec<&str> = structs.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"KeyValue"), "missing KeyValue struct, got: {names:?}");
    }

    #[test]
    fn test_load_index_header_inheritance() {
        let idx = load_index("/workspace/build", "/workspace/samples/cpp/shapes.h", None)
            .expect("load_index failed for shapes.h");
        // Circle derives from Shape
        let circle_id = idx.symbols.iter()
            .find(|e| e.name == "Circle" && e.entity == "class")
            .map(|e| e.symbol_id.clone())
            .expect("Circle not found");
        let bases = idx.bases_by_derived.get(&circle_id);
        assert!(bases.is_some(), "Circle should have base classes");
        assert!(!bases.unwrap().is_empty());
    }
}
