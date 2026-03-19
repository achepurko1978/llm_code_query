#!/usr/bin/env python3
from __future__ import annotations

import argparse, json, sys
from pathlib import Path
from typing import Any

try:
    from clang import cindex
except ModuleNotFoundError:
    cindex = None


def die(msg: str) -> None:
    raise SystemExit(msg)


def norm(p: str) -> str:
    return str(Path(p).expanduser().resolve())


def ok_base() -> dict[str, Any]:
    return {"status": "ok", "warnings": []}


def error_base(code: str, message: str) -> dict[str, Any]:
    return {"status": "error", "warnings": [{"code": code, "message": message}]}


def compile_args(build_dir: str, src: str) -> list[str]:
    if cindex is None:
        die("missing dependency: install Python clang bindings with 'sudo apt-get install -y python3-clang'")
    db = cindex.CompilationDatabase.fromDirectory(build_dir)
    cmds = db.getCompileCommands(src)
    cmd = next(iter(cmds or []), None)
    if not cmd:
        die(f"no compile command for {src}")
    args = list(cmd.arguments)[1:]
    out, skip = [], False
    paired = {"-o", "-MF", "-MT", "-MQ", "-MJ", "-Xclang", "-imacros", "-isysroot", "-target", "--target", "-ivfsoverlay"}
    single = {"-c", "-M", "-MM", "-MD", "-MMD", "-MP", "-Winvalid-pch"}
    srcs = {norm(src), norm(str(Path(cmd.directory) / cmd.filename))}
    for a in args:
        if skip:
            skip = False
            continue
        if a in single:
            continue
        if a in paired:
            skip = True
            continue
        if a.startswith(("-o", "-MF", "-MT", "-MQ", "-MJ")) and a not in paired:
            continue
        if not a.startswith("-") and norm(str(Path(cmd.directory) / a)) in srcs:
            continue
        out.append(a)
    return out


def parse(build_dir: str, src: str):
    if cindex is None:
        die("missing dependency: install Python clang bindings with 'sudo apt-get install -y python3-clang'")
    return cindex.Index.create().parse(src, args=compile_args(build_dir, src))


def same_file(cur, src: str) -> bool:
    f = cur.location.file
    return bool(f and norm(f.name) == norm(src))


def functions(tu, src: str):
    if cindex is None:
        die("missing dependency: install Python clang bindings with 'sudo apt-get install -y python3-clang'")
    for c in tu.cursor.get_children():
        if c.kind == cindex.CursorKind.FUNCTION_DECL and c.is_definition() and same_file(c, src):
            yield c


def t(typ) -> str:
    return (typ.spelling or "").strip()


def sig(c) -> str:
    ps = []
    for p in c.get_arguments() or []:
        s = t(p.type)
        if p.spelling:
            s += f" {p.spelling}"
        ps.append(s)
    base = f"{t(c.result_type)} {c.spelling}({', '.join(ps)})"
    is_const = hasattr(c, "is_const_method") and c.is_const_method()
    return f"{base} const" if is_const else base


def qualified_name(c) -> str:
    parts = []
    cur = c
    while cur and cur.kind != cindex.CursorKind.TRANSLATION_UNIT:
        if cur.spelling:
            parts.append(cur.spelling)
        cur = cur.semantic_parent
    return "::".join(reversed(parts))


def symbol_id(c) -> str:
    loc = c.location
    f = loc.file.name if loc and loc.file else "<unknown>"
    return c.get_usr() or f"loc:{f}:{loc.line}:{loc.column}"


def entity_of(c) -> str | None:
    k = c.kind
    m = {
        cindex.CursorKind.CLASS_DECL: "class",
        cindex.CursorKind.STRUCT_DECL: "struct",
        cindex.CursorKind.FUNCTION_DECL: "function",
        cindex.CursorKind.CXX_METHOD: "method",
        cindex.CursorKind.CONSTRUCTOR: "constructor",
        cindex.CursorKind.DESTRUCTOR: "destructor",
        cindex.CursorKind.FIELD_DECL: "field",
        cindex.CursorKind.VAR_DECL: "variable",
        cindex.CursorKind.PARM_DECL: "parameter",
        cindex.CursorKind.CALL_EXPR: "call",
        cindex.CursorKind.ENUM_DECL: "enum",
        cindex.CursorKind.NAMESPACE: "namespace",
    }
    return m.get(k)


def location_dict(c) -> dict[str, Any]:
    loc = c.location
    out = {"file": "<unknown>"}
    if loc and loc.file:
        out["file"] = str(loc.file.name)
    if loc and loc.line:
        out["line"] = loc.line
    if loc and loc.column:
        out["column"] = loc.column
    return out


def access_of(c) -> str | None:
    if not hasattr(c, "access_specifier"):
        return None
    a = c.access_specifier
    if a == cindex.AccessSpecifier.PUBLIC:
        return "public"
    if a == cindex.AccessSpecifier.PROTECTED:
        return "protected"
    if a == cindex.AccessSpecifier.PRIVATE:
        return "private"
    return None


def bool_attr(c, name: str) -> bool | None:
    if hasattr(c, name):
        try:
            return bool(getattr(c, name)())
        except Exception:
            return None
    return None


def parameter_summary(p, pos: int) -> dict[str, Any]:
    out: dict[str, Any] = {
        "entity": "parameter",
        "name": p.spelling or "",
        "type": t(p.type),
        "position": pos,
    }
    sid = symbol_id(p)
    if sid:
        out["symbol_id"] = sid
    out["location"] = location_dict(p)
    return out


def symbol_summary(c) -> dict[str, Any]:
    e = entity_of(c)
    nm = c.spelling or ""
    if e == "call" and not nm:
        ref = c.referenced
        if ref and ref.spelling:
            nm = ref.spelling
        elif c.displayname:
            nm = c.displayname

    out: dict[str, Any] = {
        "symbol_id": symbol_id(c),
        "entity": e,
        "name": nm,
    }
    qn = ""
    if e == "call":
        ref = c.referenced
        if ref:
            qn = qualified_name(ref)
    else:
        qn = qualified_name(c)
    if qn:
        out["qualified_name"] = qn

    if e in {"function", "method", "constructor", "destructor"}:
        out["signature"] = sig(c)
        rt = t(c.result_type)
        if rt:
            out["return_type"] = rt
        args = list(c.get_arguments() or [])
        out["parameters"] = [parameter_summary(p, i) for i, p in enumerate(args)]
        b = bool_attr(c, "is_static_method")
        if b is not None:
            out["static"] = b
        b = bool_attr(c, "is_const_method")
        if b is not None:
            out["const"] = b
        b = bool_attr(c, "is_virtual_method")
        if b is not None:
            out["virtual"] = b
        if e == "method":
            try:
                out["override"] = len(list(c.get_overridden_cursors() or [])) > 0
            except Exception:
                out["override"] = False
        b = bool_attr(c, "is_pure_virtual_method")
        if b is not None and b:
            out["virtual"] = True
    else:
        ty = t(c.type)
        if ty:
            out["type"] = ty

    acc = access_of(c)
    if acc:
        out["access"] = acc

    b = bool_attr(c, "is_deleted_method")
    if b is not None:
        out["deleted"] = b
    b = bool_attr(c, "is_default_method")
    if b is not None:
        out["defaulted"] = b
    b = bool_attr(c, "is_implicit")
    if b is not None:
        out["implicit"] = b

    out["location"] = location_dict(c)
    return out


def relation_summary(kind: str, c) -> dict[str, Any]:
    s = symbol_summary(c)
    out = {
        "kind": kind,
        "symbol_id": s["symbol_id"],
        "entity": s["entity"],
        "name": s["name"],
    }
    for k in ("qualified_name", "signature", "location"):
        if k in s:
            out[k] = s[k]
    return out


def walk(c):
    yield c
    for ch in c.get_children():
        yield from walk(ch)


# Paths containing these segments are considered external (system/vendor/generated).
_EXTERNAL_SEGMENTS: tuple[str, ...] = (
    "/.conan2/",
    "/_deps/",
    "/usr/include",
    "/usr/lib",
    "/usr/local/include",
)


def is_in_file(c, src: str) -> bool:
    loc = c.location
    return bool(loc and loc.file and norm(loc.file.name) == norm(src))


def is_workspace_file(file_path: str, workspace_root: str) -> bool:
    """True if file_path is under workspace_root and not in an external subtree."""
    n = norm(file_path)
    if not n.startswith(workspace_root):
        return False
    for seg in _EXTERNAL_SEGMENTS:
        if seg in n:
            return False
    return True


class IndexData:
    def __init__(self, tu, src: str):
        self.tu = tu
        self.src = src
        self.symbols: list[Any] = []
        self.by_id: dict[str, Any] = {}
        self.calls_by_caller: dict[str, list[str]] = {}
        self.called_by_target: dict[str, list[str]] = {}
        self.bases_by_derived: dict[str, list[str]] = {}
        self.overrides_by_method: dict[str, list[str]] = {}
        self.contains_by_parent: dict[str, list[str]] = {}


def build_index(tu, src: str, workspace_root: str | None = None) -> IndexData:
    idx = IndexData(tu, src)

    def in_scope(c) -> bool:
        loc = c.location
        if not (loc and loc.file):
            return False
        if workspace_root:
            return is_workspace_file(loc.file.name, workspace_root)
        return norm(loc.file.name) == norm(src)

    for c in walk(tu.cursor):
        if c.kind == cindex.CursorKind.TRANSLATION_UNIT:
            continue
        e = entity_of(c)
        if not e:
            continue
        if not in_scope(c):
            continue
        sid = symbol_id(c)
        idx.symbols.append(c)
        idx.by_id[sid] = c

    for c in idx.symbols:
        sid = symbol_id(c)
        parent = c.semantic_parent
        if parent and entity_of(parent) in {"class", "struct", "namespace"} and in_scope(parent):
            idx.contains_by_parent.setdefault(symbol_id(parent), []).append(sid)

        if entity_of(c) in {"class", "struct"}:
            for ch in c.get_children():
                if ch.kind == cindex.CursorKind.CXX_BASE_SPECIFIER:
                    base = ch.referenced
                    if base and entity_of(base) in {"class", "struct"}:
                        idx.bases_by_derived.setdefault(sid, []).append(symbol_id(base))

        if entity_of(c) in {"method", "constructor", "destructor"}:
            try:
                for ov in c.get_overridden_cursors() or []:
                    idx.overrides_by_method.setdefault(sid, []).append(symbol_id(ov))
            except Exception:
                pass

        if entity_of(c) in {"function", "method", "constructor", "destructor"}:
            seen = set()
            for ch in walk(c):
                if ch.kind != cindex.CursorKind.CALL_EXPR:
                    continue
                tgt = ch.referenced
                if not tgt:
                    continue
                te = entity_of(tgt)
                if te not in {"function", "method", "constructor", "destructor"}:
                    continue
                tid = symbol_id(tgt)
                if tid in seen:
                    continue
                seen.add(tid)
                idx.calls_by_caller.setdefault(sid, []).append(tid)
                idx.called_by_target.setdefault(tid, []).append(sid)

    return idx


def item(c) -> dict:
    return symbol_summary(c)


def list_functions(build_dir: str, src: str) -> dict:
    xs = [item(c) for c in functions(parse(build_dir, src), src)]
    return {"status": "ok", "result_kind": "list", "items": xs, "warnings": [], "page": {"next_cursor": None, "truncated": False, "total_matches": len(xs)}}


def describe_function(build_dir: str, src: str, name: str) -> dict:
    ms = [c for c in functions(parse(build_dir, src), src) if c.spelling == name]
    if not ms:
        return {"status": "ok", "result_kind": "describe_symbol", "item": None, "warnings": [{"code": "NO_MATCH", "message": f"no function named {name}"}]}
    if len(ms) > 1:
        return {"status": "ok", "result_kind": "describe_symbol", "item": None, "warnings": [{"code": "AMBIGUOUS_SYMBOL", "message": f"multiple functions named {name}"}], "candidates": [item(c) for c in ms]}
    c = ms[0]
    out = item(c)
    out["parameters"] = [{"name": p.spelling or "", "type": t(p.type), "position": i} for i, p in enumerate(c.get_arguments() or [])]
    return {"status": "ok", "result_kind": "describe_symbol", "item": out, "warnings": []}


def parse_request(req_json: str | None, req_file: str | None) -> dict[str, Any]:
    if req_json:
        try:
            return json.loads(req_json)
        except json.JSONDecodeError as e:
            die(f"invalid JSON in --request-json: {e}")
    if req_file:
        try:
            with open(req_file, "r", encoding="utf-8") as f:
                return json.load(f)
        except FileNotFoundError:
            die(f"request file not found: {req_file}")
        except json.JSONDecodeError as e:
            die(f"invalid JSON in --request-file {req_file}: {e}")
    die("one of --request-json or --request-file is required")


def parse_cursor(s: str | None) -> int:
    if not s:
        return 0
    try:
        x = int(s)
        return x if x >= 0 else 0
    except Exception:
        return 0


def page_slice(items: list[dict[str, Any]], limit: int, cursor: str | None) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    off = parse_cursor(cursor)
    total = len(items)
    xs = items[off: off + limit]
    nxt = off + len(xs)
    truncated = nxt < total
    page = {"next_cursor": str(nxt) if truncated else None, "truncated": truncated, "total_matches": total}
    return xs, page


def callable_param_types(c) -> list[str]:
    if entity_of(c) not in {"function", "method", "constructor", "destructor"}:
        return []
    return [t(p.type) for p in (c.get_arguments() or [])]


def relation_match(idx: IndexData, sid: str, where_rel: dict[str, Any]) -> bool:
    for k, v in where_rel.items():
        if k == "derives_from":
            vals = idx.bases_by_derived.get(sid, [])
        elif k == "overrides":
            vals = idx.overrides_by_method.get(sid, [])
        elif k == "calls":
            vals = idx.calls_by_caller.get(sid, [])
        elif k == "called_by":
            vals = idx.called_by_target.get(sid, [])
        else:
            continue

        if v in vals:
            continue

        # allow qualified-name matching fallback
        names = set()
        for rid in vals:
            rc = idx.by_id.get(rid)
            if rc:
                names.add(qualified_name(rc))
        if v not in names:
            return False
    return True


def passes_where(idx: IndexData, c, where: dict[str, Any] | None) -> bool:
    if not where:
        return True

    sid = symbol_id(c)
    s = symbol_summary(c)

    for key in (
        "name",
        "qualified_name",
        "return_type",
        "type",
        "static",
        "const",
        "virtual",
        "override",
        "deleted",
        "defaulted",
        "implicit",
        "access",
    ):
        if key in where and s.get(key) != where[key]:
            return False

    if "param_types" in where and callable_param_types(c) != list(where.get("param_types") or []):
        return False

    rel = where.get("relations")
    if rel and not relation_match(idx, sid, rel):
        return False

    any_of = where.get("any_of")
    if any_of:
        if not any(passes_where(idx, c, cond if isinstance(cond, dict) else None) for cond in any_of):
            return False

    return True


def scope_ancestors(c) -> list[Any]:
    out = []
    cur = c.semantic_parent
    while cur and cur.kind != cindex.CursorKind.TRANSLATION_UNIT:
        out.append(cur)
        cur = cur.semantic_parent
    return out


def passes_scope(c, scope: dict[str, Any] | None) -> bool:
    if not scope:
        return True
    if "file" in scope:
        loc = c.location
        if not (loc and loc.file and norm(loc.file.name) == norm(scope["file"])):
            return False

    ancestors = scope_ancestors(c)

    if "inside_function" in scope:
        want = scope["inside_function"]
        ok = False
        for a in ancestors:
            if entity_of(a) in {"function", "method", "constructor", "destructor"} and symbol_id(a) == want:
                ok = True
                break
        if not ok:
            return False

    if "inside_class" in scope:
        want = scope["inside_class"]
        ok = False
        for a in ancestors:
            if entity_of(a) in {"class", "struct"} and symbol_id(a) == want:
                ok = True
                break
        if not ok:
            return False

    if "in_namespace" in scope:
        want = scope["in_namespace"]
        ok = False
        for a in ancestors:
            if entity_of(a) == "namespace":
                if symbol_id(a) == want or qualified_name(a) == want or a.spelling == want:
                    ok = True
                    break
        if not ok:
            return False

    return True


def add_relations(idx: IndexData, s: dict[str, Any], include_relations: bool, relation_limit: int) -> dict[str, Any]:
    if not include_relations:
        return s

    sid = s["symbol_id"]
    rels: dict[str, Any] = {}

    def rel_list(kind: str, ids: list[str]) -> list[dict[str, Any]]:
        out = []
        for rid in ids[:relation_limit]:
            rc = idx.by_id.get(rid)
            if rc:
                out.append(relation_summary(kind, rc))
        return out

    calls = rel_list("calls", idx.calls_by_caller.get(sid, []))
    if calls:
        rels["calls"] = calls
    called_by = rel_list("called_by", idx.called_by_target.get(sid, []))
    if called_by:
        rels["called_by"] = called_by
    bases = rel_list("derives_from", idx.bases_by_derived.get(sid, []))
    if bases:
        rels["derives_from"] = bases
    ovs = rel_list("overrides", idx.overrides_by_method.get(sid, []))
    if ovs:
        rels["overrides"] = ovs
    cont = rel_list("contains", idx.contains_by_parent.get(sid, []))
    if cont:
        rels["contains"] = cont

    if rels:
        s = dict(s)
        s["relations"] = rels
    return s


def tool_cpp_resolve_symbol(idx: IndexData, req: dict[str, Any]) -> dict[str, Any]:
    name = req.get("name")
    if not name:
        out = error_base("INVALID_REQUEST", "name is required")
        out.update({"result_kind": "resolve_symbol", "ambiguous": False, "items": [], "page": {"next_cursor": None, "truncated": False, "total_matches": 0}})
        return out

    limit = int(req.get("limit", 20))
    limit = max(1, min(limit, 100))

    exact = []
    fuzzy = []
    nlow = str(name).lower()
    for c in idx.symbols:
        s = symbol_summary(c)
        if s["name"] == name:
            exact.append((c, s))
        elif nlow in s["name"].lower() or nlow in s.get("qualified_name", "").lower():
            fuzzy.append((c, s))

    candidates = exact if exact else fuzzy

    entity = req.get("entity")
    qn = req.get("qualified_name")
    f = req.get("file")
    param_types = req.get("param_types")

    filtered = []
    for c, s in sorted(candidates, key=lambda x: x[1].get("qualified_name", x[1]["name"])):
        if entity and s.get("entity") != entity:
            continue
        if qn and s.get("qualified_name") != qn:
            continue
        if f:
            lf = s.get("location", {}).get("file")
            if not (lf and norm(lf) == norm(f)):
                continue
        if param_types is not None and callable_param_types(c) != list(param_types):
            continue
        filtered.append(s)

    items, page = page_slice(filtered, limit, None)
    return {"status": "ok", "result_kind": "resolve_symbol", "ambiguous": len(filtered) > 1, "items": items, "warnings": [], "page": page}


def tool_cpp_semantic_query(idx: IndexData, req: dict[str, Any]) -> dict[str, Any]:
    action = req.get("action")
    entity = req.get("entity")
    if action not in {"find", "list", "count", "exists"}:
        out = error_base("INVALID_REQUEST", "action must be one of find|list|count|exists")
        out["result_kind"] = action if isinstance(action, str) else "list"
        return out
    if not entity:
        out = error_base("INVALID_REQUEST", "entity is required")
        out["result_kind"] = action
        return out

    scope = req.get("scope") if isinstance(req.get("scope"), dict) else None
    where = req.get("where") if isinstance(req.get("where"), dict) else None
    limit = int(req.get("limit", 100))
    limit = max(1, min(limit, 1000))
    cursor = req.get("cursor")
    fields: list[str] | None = req.get("fields") if isinstance(req.get("fields"), list) else None

    if entity == "file":
        file_item = {
            "symbol_id": f"file:{norm(idx.src)}",
            "entity": "file",
            "name": Path(idx.src).name,
            "qualified_name": norm(idx.src),
            "location": {"file": norm(idx.src)},
        }
        matches = [file_item]
        if where:
            if "name" in where and file_item["name"] != where["name"]:
                matches = []
            if "qualified_name" in where and file_item["qualified_name"] != where["qualified_name"]:
                matches = []
    else:
        matches = []
        for c in idx.symbols:
            if entity_of(c) != entity:
                continue
            if not passes_scope(c, scope):
                continue
            if not passes_where(idx, c, where):
                continue
            matches.append(symbol_summary(c))

    if action in {"find", "list"}:
        items, page = page_slice(matches, limit, cursor)
        if fields:
            keep = set(fields)
            items = [{k: v for k, v in item.items() if k in keep} for item in items]
        return {"status": "ok", "result_kind": action, "items": items, "warnings": [], "page": page}
    if action == "count":
        return {"status": "ok", "result_kind": "count", "count": len(matches), "warnings": []}
    return {"status": "ok", "result_kind": "exists", "exists": bool(matches), "warnings": []}


def tool_cpp_describe_symbol(idx: IndexData, req: dict[str, Any]) -> dict[str, Any]:
    sid = req.get("symbol_id")
    if not sid:
        out = error_base("INVALID_REQUEST", "symbol_id is required")
        out.update({"result_kind": "describe_symbol", "item": {"symbol_id": "", "entity": "file", "name": ""}})
        return out

    include_relations = bool(req.get("include_relations", True))
    relation_limit = int(req.get("relation_limit", 20))
    relation_limit = max(0, min(relation_limit, 100))

    c = idx.by_id.get(sid)
    if not c:
        return {
            "status": "ok",
            "result_kind": "describe_symbol",
            "item": {"symbol_id": sid, "entity": "file", "name": ""},
            "warnings": [{"code": "NO_MATCH", "message": f"symbol not found: {sid}"}],
        }

    s = symbol_summary(c)
    s = add_relations(idx, s, include_relations, relation_limit)
    return {"status": "ok", "result_kind": "describe_symbol", "item": s, "warnings": []}


def load_index(build_dir: str, src: str, workspace_root: str | None = None) -> IndexData:
    tu = parse(build_dir, src)
    return build_index(tu, src, workspace_root)


def doctor(build_dir: str | None, src: str | None) -> dict:
    checks = []

    if cindex is None:
        checks.append({"name": "python_clang_module", "ok": False, "message": "missing Python clang bindings"})
    else:
        checks.append({"name": "python_clang_module", "ok": True, "message": "clang.cindex import succeeded"})
        try:
            cindex.Index.create()
            checks.append({"name": "libclang_runtime", "ok": True, "message": "libclang runtime is usable"})
        except Exception as e:
            checks.append({"name": "libclang_runtime", "ok": False, "message": f"libclang unavailable: {e}"})

    if build_dir:
        p = Path(build_dir)
        db_path = p / "compile_commands.json"
        checks.append({"name": "build_dir_exists", "ok": p.is_dir(), "message": str(p)})
        checks.append({"name": "compile_commands_json", "ok": db_path.is_file(), "message": str(db_path)})

    if src:
        p = Path(src)
        checks.append({"name": "source_file_exists", "ok": p.is_file(), "message": str(p)})

    ok = all(c["ok"] for c in checks)
    warnings = [] if ok else [{"code": "CHECK_FAILED", "message": "one or more doctor checks failed"}]
    return {"status": "ok", "result_kind": "doctor", "ok": ok, "checks": checks, "warnings": warnings}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--build-dir")
    ap.add_argument("--file")
    ap.add_argument("--workspace-root")
    sp = ap.add_subparsers(dest="cmd", required=True)
    sp.add_parser("list-functions")
    sp.add_parser("doctor")

    p = sp.add_parser("describe-function")
    p.add_argument("--name", required=True)

    p = sp.add_parser("cpp_resolve_symbol")
    p.add_argument("--request-json")
    p.add_argument("--request-file")

    p = sp.add_parser("cpp_semantic_query")
    p.add_argument("--request-json")
    p.add_argument("--request-file")

    p = sp.add_parser("cpp_describe_symbol")
    p.add_argument("--request-json")
    p.add_argument("--request-file")

    if len(sys.argv) == 1:
        ap.print_help(sys.stdout)
        return 0

    a = ap.parse_args()
    src = norm(a.file) if a.file else None
    build = norm(a.build_dir) if a.build_dir else None
    ws_root = norm(a.workspace_root) if getattr(a, "workspace_root", None) else None

    if a.cmd == "doctor":
        out = doctor(build, src)
    elif a.cmd in {"cpp_resolve_symbol", "cpp_semantic_query", "cpp_describe_symbol"}:
        if not build or not src:
            die("--build-dir and --file are required for cpp_* tool commands")
        try:
            req = parse_request(getattr(a, "request_json", None), getattr(a, "request_file", None))
            idx = load_index(build, src, ws_root)
            if a.cmd == "cpp_resolve_symbol":
                out = tool_cpp_resolve_symbol(idx, req)
            elif a.cmd == "cpp_semantic_query":
                out = tool_cpp_semantic_query(idx, req)
            else:
                out = tool_cpp_describe_symbol(idx, req)
        except SystemExit:
            raise
        except Exception as e:
            kind = {
                "cpp_resolve_symbol": "resolve_symbol",
                "cpp_semantic_query": "list",
                "cpp_describe_symbol": "describe_symbol",
            }[a.cmd]
            out = error_base("INTERNAL_ERROR", str(e))
            out["result_kind"] = kind
            if kind == "resolve_symbol":
                out["ambiguous"] = False
                out["items"] = []
                out["page"] = {"next_cursor": None, "truncated": False, "total_matches": 0}
            elif kind == "list":
                out["items"] = []
                out["page"] = {"next_cursor": None, "truncated": False, "total_matches": 0}
            else:
                out["item"] = {"symbol_id": "", "entity": "file", "name": ""}
    else:
        if not build or not src:
            die("--build-dir and --file are required for list-functions and describe-function")
        out = list_functions(build, src) if a.cmd == "list-functions" else describe_function(build, src, a.name)
    json.dump(out, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
