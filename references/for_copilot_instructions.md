## clang-cpp / MCP

- Prefer `clang-cpp` MCP tools over grep or raw text when symbol identity, overloads, callers,
  callees, inheritance, overrides, scope, or ownership-relevant structure matters.
- Do not guess C++ semantics from text if `clang-cpp` is available.
- Use:
  - `cpp_semantic_query` for all structural, relational, and search queries
- Prefer scoped queries with `scope.path` for fast, targeted results.
- Do not silently collapse ambiguous matches or overloads.
- If MCP fails, say so explicitly, then fall back to text search only if needed.
- Only request the fields explicitly mentioned in the user's request.
- **Inverse-relation queries (`called_by`, broad `overrides`)**: ALWAYS grep/ripgrep for the
  function name first to find candidate files, then run `cpp_semantic_query` scoped to only
  those files. Even moderate scopes (50+ files) are noticeably slow without this. Rule of
  thumb: if scope could match more than ~20 files, grep first.

---