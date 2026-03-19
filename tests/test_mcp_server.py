"""Comprehensive tests for mcp_server.py — SDK-based implementation."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any
from unittest import mock

import anyio
import pytest
from mcp import types

import mcp_server as srv


# ---------------------------------------------------------------------------
# _resolve_refs
# ---------------------------------------------------------------------------

class TestResolveRefs:
    def test_simple_ref(self):
        defs = {"Foo": {"type": "string"}}
        assert srv._resolve_refs({"$ref": "#/$defs/Foo"}, defs) == {"type": "string"}

    def test_external_ref_prefix(self):
        defs = {"Foo": {"type": "integer"}}
        assert srv._resolve_refs({"$ref": "cpp-mcp-v1.schema.json#/$defs/Foo"}, defs) == {"type": "integer"}

    def test_unknown_ref_preserved(self):
        assert srv._resolve_refs({"$ref": "other.json#/bar"}, {}) == {"$ref": "other.json#/bar"}

    def test_nested(self):
        defs = {"A": {"inner": {"$ref": "#/$defs/B"}}, "B": {"val": 1}}
        assert srv._resolve_refs({"$ref": "#/$defs/A"}, defs) == {"inner": {"val": 1}}

    def test_list(self):
        defs = {"X": {"type": "boolean"}}
        assert srv._resolve_refs([{"$ref": "#/$defs/X"}, 42], defs) == [{"type": "boolean"}, 42]

    def test_strips_additional_properties(self):
        obj = {"type": "object", "additionalProperties": False, "name": "x"}
        result = srv._resolve_refs(obj, {})
        assert "additionalProperties" not in result
        assert result["name"] == "x"

    def test_depth_limit(self):
        defs = {"Loop": {"$ref": "#/$defs/Loop"}}
        result = srv._resolve_refs({"$ref": "#/$defs/Loop"}, defs)
        assert isinstance(result, dict)

    def test_scalars_pass_through(self):
        assert srv._resolve_refs(42, {}) == 42
        assert srv._resolve_refs("hello", {}) is not None
        assert srv._resolve_refs(None, {}) is None


# ---------------------------------------------------------------------------
# load_tools_schema
# ---------------------------------------------------------------------------

class TestLoadToolsSchema:
    def test_load(self, tmp_path: Path):
        schema = {"$defs": {"MyType": {"type": "string"}}}
        tools = {"tools": [{"name": "t", "inputSchema": {"$ref": "cpp-mcp-v1.schema.json#/$defs/MyType"}}]}
        (tmp_path / "cpp-mcp-v1.schema.json").write_text(json.dumps(schema))
        (tmp_path / "tools.json").write_text(json.dumps(tools))
        result = srv.load_tools_schema(tmp_path / "tools.json")
        assert len(result) == 1
        assert result[0]["inputSchema"] == {"type": "string"}

    def test_no_schema_file(self, tmp_path: Path):
        tools = {"tools": [{"name": "t", "inputSchema": {"type": "object"}}]}
        (tmp_path / "tools.json").write_text(json.dumps(tools))
        result = srv.load_tools_schema(tmp_path / "tools.json")
        assert len(result) == 1


# ---------------------------------------------------------------------------
# norm_path / find_compile_db / source_files_from_compile_db
# ---------------------------------------------------------------------------

class TestNormPath:
    def test_absolute(self):
        assert srv.norm_path("/a/b/c", Path("/other")) == Path("/a/b/c")

    def test_relative(self):
        assert srv.norm_path("sub/dir", Path("/ws")) == Path("/ws/sub/dir")

    def test_tilde(self, monkeypatch):
        monkeypatch.setenv("HOME", "/fakehome")
        result = srv.norm_path("~/xyz", Path("/ws"))
        assert "/fakehome" in str(result)


class TestFindCompileDb:
    def test_explicit_build_dir(self, tmp_path: Path):
        bd = tmp_path / "build"
        bd.mkdir()
        cc = bd / "compile_commands.json"
        cc.write_text("[]")
        assert srv.find_compile_db(tmp_path, str(bd)) == cc

    def test_explicit_build_dir_direct_file(self, tmp_path: Path):
        cc = tmp_path / "compile_commands.json"
        cc.write_text("[]")
        assert srv.find_compile_db(tmp_path, str(cc)) == cc

    def test_explicit_not_found(self, tmp_path: Path):
        with pytest.raises(FileNotFoundError, match="not found"):
            srv.find_compile_db(tmp_path, str(tmp_path / "nonexistent"))

    def test_auto_discovery_common(self, tmp_path: Path):
        bd = tmp_path / "build"
        bd.mkdir()
        cc = bd / "compile_commands.json"
        cc.write_text("[]")
        assert srv.find_compile_db(tmp_path, None) == cc

    def test_auto_discovery_rglob(self, tmp_path: Path):
        bd = tmp_path / "deep" / "nested"
        bd.mkdir(parents=True)
        cc = bd / "compile_commands.json"
        cc.write_text("[]")
        assert srv.find_compile_db(tmp_path, None).resolve() == cc.resolve()

    def test_auto_discovery_none_found(self, tmp_path: Path):
        with pytest.raises(FileNotFoundError, match="could not discover"):
            srv.find_compile_db(tmp_path, None)


class TestSourceFilesFromCompileDb:
    def test_basic(self, tmp_path: Path):
        ws = tmp_path / "ws"
        ws.mkdir()
        src = ws / "main.cpp"
        src.write_text("int main(){}")
        bd = ws / "build"
        bd.mkdir()
        cc = bd / "compile_commands.json"
        cc.write_text(json.dumps([{"directory": str(ws), "file": "main.cpp", "command": "g++ main.cpp"}]))
        files = srv.source_files_from_compile_db(cc, ws, bd)
        assert len(files) == 1
        assert files[0] == str(src.resolve())

    def test_skips_build_dir_files(self, tmp_path: Path):
        ws = tmp_path / "ws"
        ws.mkdir()
        bd = ws / "build"
        bd.mkdir()
        gen = bd / "gen.cpp"
        gen.write_text("// generated")
        cc = bd / "compile_commands.json"
        cc.write_text(json.dumps([{"directory": str(bd), "file": "gen.cpp", "command": "g++ gen.cpp"}]))
        assert srv.source_files_from_compile_db(cc, ws, bd) == []

    def test_deduplication(self, tmp_path: Path):
        ws = tmp_path / "ws"
        ws.mkdir()
        src = ws / "a.cpp"
        src.write_text("int a;")
        bd = ws / "build"
        bd.mkdir()
        cc = bd / "compile_commands.json"
        cc.write_text(json.dumps([
            {"directory": str(ws), "file": "a.cpp", "command": "g++ a.cpp"},
            {"directory": str(ws), "file": "a.cpp", "command": "g++ a.cpp -DFOO"},
        ]))
        assert len(srv.source_files_from_compile_db(cc, ws, bd)) == 1

    def test_non_list_db(self, tmp_path: Path):
        ws = tmp_path / "ws"
        ws.mkdir()
        bd = ws / "build"
        bd.mkdir()
        cc = bd / "compile_commands.json"
        cc.write_text(json.dumps({"entries": []}))
        assert srv.source_files_from_compile_db(cc, ws, bd) == []

    def test_filters_non_cpp(self, tmp_path: Path):
        ws = tmp_path / "ws"
        ws.mkdir()
        (ws / "script.py").write_text("pass")
        bd = ws / "build"
        bd.mkdir()
        cc = bd / "compile_commands.json"
        cc.write_text(json.dumps([{"directory": str(ws), "file": "script.py", "command": "python script.py"}]))
        assert srv.source_files_from_compile_db(cc, ws, bd) == []


# ---------------------------------------------------------------------------
# parse_int / parse_cursor / dedupe_items / is_no_match_describe
# ---------------------------------------------------------------------------

class TestParseInt:
    def test_normal(self):
        assert srv.parse_int(50, 10, 1, 100) == 50

    def test_clamp_low(self):
        assert srv.parse_int(-5, 10, 1, 100) == 1

    def test_clamp_high(self):
        assert srv.parse_int(999, 10, 1, 100) == 100

    def test_bad_value(self):
        assert srv.parse_int("abc", 10, 1, 100) == 10

    def test_none(self):
        assert srv.parse_int(None, 20, 1, 100) == 20


class TestParseCursor:
    def test_valid(self):
        assert srv.parse_cursor("5") == 5

    def test_zero(self):
        assert srv.parse_cursor(0) == 0

    def test_negative(self):
        assert srv.parse_cursor(-3) == 0

    def test_bad(self):
        assert srv.parse_cursor("nope") == 0


class TestDedupeItems:
    def test_by_symbol_id(self):
        items = [{"symbol_id": "s1", "x": 1}, {"symbol_id": "s1", "x": 2}]
        assert len(srv.dedupe_items(items)) == 1

    def test_by_json(self):
        items = [{"a": 1, "b": 2}, {"b": 2, "a": 1}]
        assert len(srv.dedupe_items(items)) == 1

    def test_different(self):
        items = [{"symbol_id": "s1"}, {"symbol_id": "s2"}]
        assert len(srv.dedupe_items(items)) == 2

    def test_empty(self):
        assert srv.dedupe_items([]) == []


class TestIsNoMatchDescribe:
    def test_no_item(self):
        assert srv.is_no_match_describe({}) is True

    def test_item_with_name(self):
        assert srv.is_no_match_describe({"item": {"name": "foo"}}) is False

    def test_no_match_warning(self):
        payload = {"item": {"name": ""}, "warnings": [{"code": "NO_MATCH", "message": "not found"}]}
        assert srv.is_no_match_describe(payload) is True

    def test_item_no_name(self):
        assert srv.is_no_match_describe({"item": {}}) is True


# ---------------------------------------------------------------------------
# target_files_for_tool
# ---------------------------------------------------------------------------

class TestTargetFilesForTool:
    def setup_method(self):
        self.all_files = ["/ws/a.cpp", "/ws/b.cpp"]
        self.ws = Path("/ws")

    def test_resolve_symbol_with_file(self, tmp_path: Path):
        f = tmp_path / "x.cpp"
        f.write_text("")
        files, is_dir = srv.target_files_for_tool("cpp_resolve_symbol", {"file": str(f)}, self.all_files, tmp_path)
        assert files == [str(f.resolve())]
        assert is_dir is False

    def test_resolve_symbol_no_file(self):
        files, is_dir = srv.target_files_for_tool("cpp_resolve_symbol", {}, self.all_files, self.ws)
        assert files == self.all_files
        assert is_dir is False

    def test_semantic_query_with_scope_file(self, tmp_path: Path):
        f = tmp_path / "y.cpp"
        f.write_text("")
        files, is_dir = srv.target_files_for_tool("cpp_semantic_query", {"scope": {"file": str(f)}}, self.all_files, tmp_path)
        assert files == [str(f.resolve())]
        assert is_dir is False

    def test_semantic_query_no_scope(self):
        files, is_dir = srv.target_files_for_tool("cpp_semantic_query", {}, self.all_files, self.ws)
        assert files == self.all_files
        assert is_dir is False

    def test_describe_symbol(self):
        files, is_dir = srv.target_files_for_tool("cpp_describe_symbol", {"symbol_id": "s1"}, self.all_files, self.ws)
        assert files == self.all_files
        assert is_dir is False

    def test_resolve_symbol_with_directory(self, tmp_path: Path):
        d = tmp_path / "src"
        d.mkdir()
        all_files = [str(d / "a.cpp"), str(d / "b.cpp"), str(tmp_path / "other.cpp")]
        files, is_dir = srv.target_files_for_tool("cpp_resolve_symbol", {"file": str(d)}, all_files, tmp_path)
        assert files == [str(d / "a.cpp"), str(d / "b.cpp")]
        assert is_dir is True

    def test_semantic_query_with_scope_directory(self, tmp_path: Path):
        d = tmp_path / "lib"
        d.mkdir()
        all_files = [str(d / "x.cpp"), str(tmp_path / "main.cpp")]
        files, is_dir = srv.target_files_for_tool("cpp_semantic_query", {"scope": {"file": str(d)}}, all_files, tmp_path)
        assert files == [str(d / "x.cpp")]
        assert is_dir is True

    def test_directory_no_matching_files(self, tmp_path: Path):
        d = tmp_path / "empty_dir"
        d.mkdir()
        files, is_dir = srv.target_files_for_tool("cpp_resolve_symbol", {"file": str(d)}, ["/other/a.cpp"], tmp_path)
        assert files == []
        assert is_dir is True


class TestStripDirScope:
    def test_semantic_query_strips_file_from_scope(self):
        args = {"action": "list", "entity": "function", "scope": {"file": "dir", "extra": "val"}}
        result = srv._strip_dir_scope(args, "cpp_semantic_query")
        assert result == {"action": "list", "entity": "function", "scope": {"extra": "val"}}

    def test_semantic_query_removes_empty_scope(self):
        args = {"action": "list", "entity": "function", "scope": {"file": "dir"}}
        result = srv._strip_dir_scope(args, "cpp_semantic_query")
        assert result == {"action": "list", "entity": "function"}
        assert "scope" not in result

    def test_resolve_symbol_strips_file(self):
        args = {"file": "dir", "name": "add"}
        result = srv._strip_dir_scope(args, "cpp_resolve_symbol")
        assert result == {"name": "add"}

    def test_does_not_mutate_original(self):
        args = {"action": "list", "scope": {"file": "dir"}}
        srv._strip_dir_scope(args, "cpp_semantic_query")
        assert args["scope"] == {"file": "dir"}

    def test_no_scope_passthrough(self):
        args = {"action": "list", "entity": "function"}
        result = srv._strip_dir_scope(args, "cpp_semantic_query")
        assert result == args

    def test_describe_symbol_passthrough(self):
        args = {"symbol_id": "s1"}
        result = srv._strip_dir_scope(args, "cpp_describe_symbol")
        assert result == args


# ---------------------------------------------------------------------------
# run_backend
# ---------------------------------------------------------------------------

class TestRunBackend:
    def test_success(self, tmp_path: Path):
        script = tmp_path / "backend.py"
        script.write_text('import sys, json; json.dump({"status": "ok", "items": []}, sys.stdout)')
        result = srv.run_backend(script, str(tmp_path), "f.cpp", "cpp_resolve_symbol", {"name": "x"}, 10)
        assert result["status"] == "ok"

    def test_nonzero_exit(self, tmp_path: Path):
        script = tmp_path / "fail.py"
        script.write_text('import sys; sys.stderr.write("boom"); sys.exit(1)')
        result = srv.run_backend(script, str(tmp_path), "f.cpp", "test_cmd", {}, 10)
        assert result["status"] == "error"
        assert result["warnings"][0]["code"] == "BACKEND_ERROR"

    def test_timeout(self, tmp_path: Path):
        script = tmp_path / "slow.py"
        script.write_text('import time; time.sleep(60)')
        result = srv.run_backend(script, str(tmp_path), "f.cpp", "test_cmd", {}, 1)
        assert result["status"] == "error"
        assert result["warnings"][0]["code"] == "BACKEND_TIMEOUT"

    def test_bad_json_output(self, tmp_path: Path):
        script = tmp_path / "bad.py"
        script.write_text('print("not json")')
        result = srv.run_backend(script, str(tmp_path), "f.cpp", "test_cmd", {}, 10)
        assert result["status"] == "error"
        assert result["warnings"][0]["code"] == "BACKEND_BAD_JSON"

    def test_binary_backend(self, tmp_path: Path):
        script = tmp_path / "backend"
        script.write_text("#!/bin/sh\necho '{}'")
        script.chmod(0o755)
        result = srv.run_backend(script, str(tmp_path), "f.cpp", "test_cmd", {}, 10)
        assert isinstance(result, dict)

    def test_workspace_root_arg(self, tmp_path: Path):
        script = tmp_path / "echo.py"
        script.write_text('import sys, json\njson.dump({"status": "ok", "args": sys.argv[1:]}, sys.stdout)\n')
        result = srv.run_backend(script, str(tmp_path), "f.cpp", "cmd", {}, 10, workspace_root="/my/ws")
        assert "--workspace-root" in result.get("args", [])
        assert "/my/ws" in result.get("args", [])


# ---------------------------------------------------------------------------
# route_tool_call
# ---------------------------------------------------------------------------

class TestRouteToolCall:
    def test_no_files(self):
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), [], "cpp_resolve_symbol", {"name": "x"}, 10)
        assert result["warnings"][0]["code"] == "NO_SOURCE_FILES"

    def test_no_target_files(self, tmp_path: Path):
        result = srv.route_tool_call(Path("x"), "bd", tmp_path, ["/ws/a.cpp"],
                                     "cpp_resolve_symbol", {"file": str(tmp_path / "nonexistent.cpp")}, 10)
        assert result["warnings"][0]["code"] == "NO_TARGET_FILES"

    @mock.patch("mcp_server.run_backend")
    def test_resolve_symbol_ok(self, mock_be):
        mock_be.return_value = {"status": "ok", "items": [{"symbol_id": "s1", "name": "foo"}], "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_resolve_symbol", {"name": "foo", "limit": 5}, 10)
        assert result["status"] == "ok"
        assert result["result_kind"] == "resolve_symbol"
        assert len(result["items"]) == 1

    @mock.patch("mcp_server.run_backend")
    def test_resolve_symbol_dedup_across_files(self, mock_be):
        mock_be.side_effect = [
            {"status": "ok", "items": [{"symbol_id": "s1"}], "warnings": []},
            {"status": "ok", "items": [{"symbol_id": "s1"}], "warnings": []},
        ]
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp", "/ws/b.cpp"],
                                     "cpp_resolve_symbol", {"name": "foo"}, 10)
        assert len(result["items"]) == 1

    @mock.patch("mcp_server.run_backend")
    def test_resolve_symbol_truncation(self, mock_be):
        mock_be.return_value = {"status": "ok", "items": [{"symbol_id": f"s{i}"} for i in range(50)], "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_resolve_symbol", {"name": "foo", "limit": 5}, 10)
        assert len(result["items"]) == 5
        assert result["page"]["truncated"] is True
        assert result["page"]["next_cursor"] == "5"

    @mock.patch("mcp_server.run_backend")
    def test_semantic_query_list(self, mock_be):
        mock_be.return_value = {"status": "ok", "items": [{"symbol_id": f"s{i}"} for i in range(10)], "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "list", "entity": "function", "limit": 5}, 10)
        assert result["result_kind"] == "list"
        assert len(result["items"]) == 5
        assert result["page"]["truncated"] is True

    @mock.patch("mcp_server.run_backend")
    def test_semantic_query_list_with_cursor(self, mock_be):
        mock_be.return_value = {"status": "ok", "items": [{"symbol_id": f"s{i}"} for i in range(10)], "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "list", "entity": "function", "limit": 5, "cursor": "3"}, 10)
        assert len(result["items"]) == 5
        assert result["page"]["next_cursor"] == "8"

    @mock.patch("mcp_server.run_backend")
    def test_semantic_query_count(self, mock_be):
        mock_be.return_value = {"status": "ok", "count": 7, "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "count", "entity": "function"}, 10)
        assert result["result_kind"] == "count"
        assert result["count"] == 7

    @mock.patch("mcp_server.run_backend")
    def test_semantic_query_exists(self, mock_be):
        mock_be.return_value = {"status": "ok", "exists": True, "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "exists", "entity": "function"}, 10)
        assert result["result_kind"] == "exists"
        assert result["exists"] is True

    @mock.patch("mcp_server.run_backend")
    def test_semantic_query_exists_false(self, mock_be):
        mock_be.return_value = {"status": "ok", "exists": False, "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "exists", "entity": "function"}, 10)
        assert result["exists"] is False

    def test_semantic_query_invalid_action(self):
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "invalid_action", "entity": "function"}, 10)
        assert result["status"] == "error"
        assert result["warnings"][0]["code"] == "INVALID_REQUEST"

    @mock.patch("mcp_server.run_backend")
    def test_describe_symbol_found(self, mock_be):
        mock_be.return_value = {"status": "ok", "item": {"symbol_id": "s1", "entity": "function", "name": "foo"}, "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_describe_symbol", {"symbol_id": "s1"}, 10)
        assert result["item"]["name"] == "foo"

    @mock.patch("mcp_server.run_backend")
    def test_describe_symbol_not_found(self, mock_be):
        mock_be.return_value = {"status": "ok", "item": {"symbol_id": "s1", "entity": "file", "name": ""},
                                "warnings": [{"code": "NO_MATCH", "message": "not found"}]}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_describe_symbol", {"symbol_id": "s1"}, 10)
        assert any(w["code"] == "NO_MATCH" for w in result["warnings"])

    @mock.patch("mcp_server.run_backend")
    def test_describe_symbol_skips_errors(self, mock_be):
        mock_be.side_effect = [
            {"status": "error", "warnings": [{"code": "BACKEND_ERROR", "message": "fail"}]},
            {"status": "ok", "item": {"symbol_id": "s1", "entity": "function", "name": "bar"}, "warnings": []},
        ]
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp", "/ws/b.cpp"],
                                     "cpp_describe_symbol", {"symbol_id": "s1"}, 10)
        assert result["item"]["name"] == "bar"

    @mock.patch("mcp_server.run_backend")
    def test_resolve_error_then_ok(self, mock_be):
        mock_be.side_effect = [
            {"status": "error", "warnings": [{"code": "BACKEND_ERROR", "message": "fail"}]},
            {"status": "ok", "items": [{"symbol_id": "s1"}], "warnings": []},
        ]
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp", "/ws/b.cpp"],
                                     "cpp_resolve_symbol", {"name": "foo"}, 10)
        assert result["status"] == "ok"
        assert len(result["items"]) == 1

    @mock.patch("mcp_server.run_backend")
    def test_resolve_all_errors(self, mock_be):
        mock_be.return_value = {"status": "error", "warnings": [{"code": "BACKEND_ERROR", "message": "fail"}]}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_resolve_symbol", {"name": "foo"}, 10)
        assert result["status"] == "error"

    @mock.patch("mcp_server.run_backend")
    def test_semantic_count_all_errors(self, mock_be):
        mock_be.return_value = {"status": "error", "count": 0, "warnings": [{"code": "E", "message": "x"}]}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "count", "entity": "function"}, 10)
        assert result["status"] == "error"

    @mock.patch("mcp_server.run_backend")
    def test_semantic_exists_all_errors(self, mock_be):
        mock_be.return_value = {"status": "error", "exists": False, "warnings": [{"code": "E", "message": "x"}]}
        result = srv.route_tool_call(Path("x"), "bd", Path("/ws"), ["/ws/a.cpp"],
                                     "cpp_semantic_query", {"action": "exists", "entity": "function"}, 10)
        assert result["status"] == "error"
        assert result["exists"] is False

    @mock.patch("mcp_server.run_backend")
    def test_directory_scope_strips_file_from_backend_args(self, mock_be, tmp_path: Path):
        """When scope.file is a directory, backend should NOT receive scope.file."""
        d = tmp_path / "src"
        d.mkdir()
        src1, src2 = str(d / "a.cpp"), str(d / "b.cpp")
        all_files = [src1, src2, str(tmp_path / "other.cpp")]
        mock_be.return_value = {"status": "ok", "items": [{"symbol_id": "s1"}], "warnings": []}
        result = srv.route_tool_call(Path("x"), "bd", tmp_path, all_files,
                                     "cpp_semantic_query",
                                     {"action": "list", "entity": "function", "scope": {"file": str(d)}}, 10)
        assert result["status"] == "ok"
        # Backend should have been called twice (once per file in dir)
        assert mock_be.call_count == 2
        # The call_args passed to backend should NOT contain scope.file
        for call in mock_be.call_args_list:
            args_obj = call[0][4]  # 5th positional arg: args_obj
            scope = args_obj.get("scope", {})
            assert "file" not in scope


# ---------------------------------------------------------------------------
# _backend_error / _tool_result
# ---------------------------------------------------------------------------

class TestHelpers:
    def test_backend_error(self):
        result = srv._backend_error("MY_CODE", "my message")
        assert result == {"status": "error", "warnings": [{"code": "MY_CODE", "message": "my message"}]}

    def test_tool_result_ok(self):
        payload = {"status": "ok", "items": []}
        r = srv._tool_result(payload)
        assert isinstance(r, types.CallToolResult)
        assert r.isError is False
        assert r.structuredContent == payload
        assert json.loads(r.content[0].text) == payload

    def test_tool_result_error(self):
        payload = {"status": "error", "warnings": []}
        r = srv._tool_result(payload)
        assert r.isError is True


# ---------------------------------------------------------------------------
# resolve_runtime_context
# ---------------------------------------------------------------------------

class TestResolveRuntimeContext:
    def test_ok(self, tmp_path: Path):
        ws = tmp_path / "ws"
        ws.mkdir()
        src = ws / "main.cpp"
        src.write_text("int main(){}")
        bd = ws / "build"
        bd.mkdir()
        cc = bd / "compile_commands.json"
        cc.write_text(json.dumps([{"directory": str(ws), "file": "main.cpp", "command": "g++ main.cpp"}]))
        build_dir, files = srv.resolve_runtime_context(ws, None)
        assert build_dir == str(bd)
        assert str(src.resolve()) in files

    def test_not_found(self, tmp_path: Path):
        with pytest.raises(FileNotFoundError):
            srv.resolve_runtime_context(tmp_path, None)


# ---------------------------------------------------------------------------
# create_server unit tests
# ---------------------------------------------------------------------------

class TestCreateServer:
    def test_server_name(self):
        server = srv.create_server([], Path("x"), Path("/ws"), None, 10)
        assert server.name == srv.SERVER_NAME

    def test_tool_result_unknown_tool(self):
        payload = srv._backend_error("UNKNOWN_TOOL", "unknown tool: bad_tool")
        result = srv._tool_result(payload)
        assert result.isError is True
        assert "UNKNOWN_TOOL" in result.content[0].text


# ---------------------------------------------------------------------------
# Full protocol integration tests via MCP SDK in-memory streams
# ---------------------------------------------------------------------------

def _make_session_message(data: dict) -> Any:
    from mcp.types import JSONRPCMessage
    from mcp.shared.message import SessionMessage
    return SessionMessage(JSONRPCMessage.model_validate(data))


class TestProtocolIntegration:
    """End-to-end tests using the MCP SDK's in-memory transport."""

    @pytest.fixture
    def server(self):
        tool_defs = [{"name": "cpp_resolve_symbol", "description": "resolve", "inputSchema": {"type": "object"}}]
        return srv.create_server(tool_defs, Path("clang_mcp.py"), Path("/workspace"), None, 10)

    def _run_session(self, server, requests: list[dict], delay: float = 0.1, timeout: float = 1.0) -> list[dict]:
        """Send requests to server and collect responses."""
        from mcp.shared.message import SessionMessage

        async def _run():
            read_send, read_recv = anyio.create_memory_object_stream[SessionMessage | Exception](10)
            write_send, write_recv = anyio.create_memory_object_stream[SessionMessage](10)
            responses: list[dict] = []

            async def collect():
                async with write_recv:
                    async for msg in write_recv:
                        responses.append(json.loads(msg.message.model_dump_json(by_alias=True, exclude_none=True)))

            async def send():
                async with read_send:
                    for req in requests:
                        await read_send.send(_make_session_message(req))
                        await anyio.sleep(delay)

            async with anyio.create_task_group() as tg:
                tg.start_soon(collect)
                tg.start_soon(send)
                tg.start_soon(server.run, read_recv, write_send, server.create_initialization_options())
                await anyio.sleep(timeout)
                tg.cancel_scope.cancel()

            return responses

        return anyio.run(_run)

    def test_initialize_and_list_tools(self, server):
        responses = self._run_session(server, [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1.0"}}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
        ])
        # initialize response
        assert responses[0]["result"]["serverInfo"]["name"] == srv.SERVER_NAME
        # tools/list response
        tools_resp = next(r for r in responses if r.get("id") == 2)
        assert len(tools_resp["result"]["tools"]) == 1
        assert tools_resp["result"]["tools"][0]["name"] == "cpp_resolve_symbol"

    @mock.patch("mcp_server.resolve_runtime_context")
    @mock.patch("mcp_server.route_tool_call")
    def test_tool_call(self, mock_route, mock_ctx, server):
        mock_ctx.return_value = ("/workspace/build", ["/workspace/samples/cpp/functions.cpp"])
        mock_route.return_value = {"status": "ok", "result_kind": "resolve_symbol",
                                   "items": [{"symbol_id": "s1"}], "warnings": [],
                                   "page": {"next_cursor": None, "truncated": False, "total_matches": 1}}
        responses = self._run_session(server, [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1.0"}}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
             "params": {"name": "cpp_resolve_symbol", "arguments": {"name": "foo"}}},
        ], timeout=1.5)
        call_resp = next((r for r in responses if r.get("id") == 3), None)
        assert call_resp is not None
        assert call_resp["result"]["structuredContent"]["status"] == "ok"

    def test_unknown_tool_via_protocol(self, server):
        responses = self._run_session(server, [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1.0"}}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
             "params": {"name": "nonexistent_tool", "arguments": {}}},
        ], timeout=1.5)
        call_resp = next((r for r in responses if r.get("id") == 2), None)
        assert call_resp is not None
        structured = call_resp["result"].get("structuredContent", {})
        assert structured.get("status") == "error"

    @mock.patch("mcp_server.resolve_runtime_context", side_effect=FileNotFoundError("no compile db"))
    def test_tool_call_context_error(self, mock_ctx, server):
        responses = self._run_session(server, [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1.0"}}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
             "params": {"name": "cpp_resolve_symbol", "arguments": {"name": "foo"}}},
        ], timeout=1.5)
        call_resp = next((r for r in responses if r.get("id") == 2), None)
        assert call_resp is not None
        structured = call_resp["result"].get("structuredContent", {})
        assert structured.get("status") == "error"
        assert any(w["code"] == "RUNTIME_CONTEXT_ERROR" for w in structured.get("warnings", []))
