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

---