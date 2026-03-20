## clang-cpp / MCP

- Prefer `clang-cpp` MCP tools over grep or raw text when symbol identity, overloads, callers,
  callees, inheritance, overrides, scope, or ownership-relevant structure matters.
- Do not guess C++ semantics from text if `clang-cpp` is available.
- Resolve symbols first when a user mentions a symbol by name.
- Use:
  - `cpp_resolve_symbol` for symbol resolution and disambiguation
  - `cpp_semantic_query` for structural and relation queries
  - `cpp_describe_symbol` for a concise semantic summary before editing unfamiliar code
- Prefer exact symbol identity over bare-name matching once resolved.
- Do not silently collapse ambiguous matches or overloads.
- If MCP fails, say so explicitly, then fall back to text search only if needed.
- Only request the fields explicitly mentioned in the user's request.

---