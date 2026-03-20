---
name: clang-cpp
description: Use the local clang-cpp MCP server for semantic queries over this C++ codebase. Always prefer these tools over grep or raw text reasoning when symbol identity, call relationships, inheritance, overrides, or scoped structural queries are involved.
---

# Purpose

Use this skill when working with a C++ codebase that exposes the following MCP tools:

- `cpp_resolve_symbol`
- `cpp_semantic_query`
- `cpp_describe_symbol`

This skill teaches the agent:

- when to use each tool
- how to sequence tool calls
- how to construct precise `cpp_semantic_query` inputs
- how to write complex `where` clauses accurately
- when to fall back safely if the MCP server cannot answer

# Core rules

- Always prefer the MCP tools over grep, text search, or AST guessing.
- Never infer symbol identity from text alone when `cpp_resolve_symbol` can disambiguate.
- When the user names a symbol, resolve it first before performing operations.
- When a query depends on relationships, scope, inheritance, or overrides, use `cpp_semantic_query`.
- When the user wants a summary of one symbol before editing or reasoning about it, use `cpp_describe_symbol`.
- If MCP fails, state this explicitly, then fall back to text search only if necessary.
- Treat `symbol_id` as an opaque identifier—never parse or construct it manually.
- Prefer exact symbol-based queries over loose name-based queries once a symbol has been resolved.

# Tool selection

## 1) `cpp_resolve_symbol`

Use when:

- the user names a function, method, class, namespace, enum, field, parameter, or variable
- the name may be overloaded or ambiguous
- the same name may exist in multiple namespaces or classes
- a subsequent semantic query must target one exact symbol

Typical uses:

- resolve `foo`
- resolve `ns::foo`
- resolve `MyClass::bar`
- resolve overloaded `push_back`
- resolve `main`

Use this before relation queries such as:

- callers / callees
- overrides
- base / derived relationships
- queries scoped to a function or class

## 2) `cpp_semantic_query`

Use when the user wants:

- find/list/count/exists operations
- callers or callees
- methods or fields of a class
- inheritance relationships
- override relationships
- items inside a function or class
- queries filtered by type, return type, access, const/static/virtual/override
- scoped structural searches
- retrieving full source bodies in bulk (set `"include_source": true`)

This is the primary workhorse tool.

## 3) `cpp_describe_symbol`

Use when the user wants:

- "what is this symbol?"
- a normalized summary before editing
- one bounded semantic summary of a class, function, or method
- a concise explanation of signature, location, and key relations
- the full source body of a function, method, or class (set `"include_source": true`)

# Query strategy

## Resolve-first pattern

Use this pattern when a user provides a symbol name and then asks a relation question.

Example:

User asks: "Find all callers of `foo`."

Preferred flow:

1. call `cpp_resolve_symbol` for `foo`
2. inspect candidates
3. if ambiguous, select the most contextually appropriate candidate only if context clearly indicates which one
4. call `cpp_semantic_query` using the resolved symbol identity or an exact qualified target

## Scope-first pattern

Use this when the user asks for things "inside" a file, directory, or matching a glob pattern.

Examples:

- all functions in `src/parse.cpp`
- methods in all headers: `include/*.h`
- symbols in a directory: `src/`

Preferred flow:

1. use `scope.path` with a file path, directory, or glob
2. add `where` filters for the specific thing being searched

Note: `scope.inside_class`, `scope.inside_function`, and `scope.in_namespace` are **not implemented** (silently ignored). To find items in a specific class or function, scope to the file and filter by `qualified_name` prefix client-side.

## Narrow-then-relate pattern

For broader requests:

- first constrain by entity and scope
- then add relation and filters
- then request only the fields you need

# Semantic model

The semantic query surface is designed around these concepts:

## Actions

- `find`
- `list`
- `count`
- `exists`

Use:

- `find` for relation-driven or filtered search
- `list` for direct membership or inventory
- `count` when only the number matters
- `exists` when only a boolean result matters

## Entities

Common entities include:

- `class`
- `struct`
- `method`
- `constructor`
- `field`
- `function`
- `variable`
- `parameter`
- `call`
- `enum`
- `namespace`
- `file`

## Relations

Relation filters are accessed through `where.relations`:

- `derives_from`
- `overrides`
- `calls`
- `called_by`

Usage: `"where": {"relations": {"derives_from": "QualifiedName"}}`

**Not implemented (silently ignored):**

- `scope.inside_function` — use `scope.path` to narrow to a file, then filter client-side
- `scope.inside_class` — same workaround
- `scope.in_namespace` — same workaround

## Filters

Supported `where` filter keys (multiple flat keys are AND'd together):

- `name` — exact string match
- `qualified_name` — exact string match
- `return_type` — exact string match
- `type` — exact string match
- `access` — `"public"`, `"protected"`, `"private"`
- `static` — boolean
- `const` — boolean
- `virtual` — boolean
- `override` — boolean
- `deleted` — boolean
- `defaulted` — boolean
- `implicit` — boolean
- `param_types` — exact array match (e.g. `["int", "const std::string &"]`)
- `any_of` — array of sub-conditions, at least one must match
- `relations` — object with relation keys (`derives_from`, `overrides`, `calls`, `called_by`)

**Not implemented (silently ignored):**

- `all_of` — use flat keys instead (they are AND'd automatically)
- `not` — filter client-side or restructure the query
- Top-level `derives_from`, `calls`, `overrides`, `called_by` — must be nested under `relations`
- `contains`
- Regex/pattern matching in values

# Preferred output discipline

Request only the fields you need.

Typical field sets:

## For lookup lists

```json
["symbol_id", "entity", "name", "qualified_name", "signature", "location"]
```

## For call relationships

```json
["symbol_id", "entity", "qualified_name", "location", "source_excerpt"]
```

## For source body retrieval

Pass `"include_source": true` in the request (works with both `cpp_semantic_query` and `cpp_describe_symbol`). The response will include:

- `"source"` — full source text (declaration + body)
- `"extent"` — `{"start_line": N, "end_line": M}`

Example field set when you need source:

```json
["symbol_id", "qualified_name", "signature", "source", "extent"]
```

## For class members

```json
["symbol_id", "entity", "name", "qualified_name", "type", "access", "location"]
```

## For summaries

```json
["symbol_id", "entity", "qualified_name", "signature", "summary", "relations", "location"]
```

# General query construction guidance

## Prefer exactness

Prefer:

- resolved symbol identity
- qualified names
- explicit entity
- explicit scope
- explicit relation

Over:

- loose names
- text fragments
- broad queries with no scope

## Keep `where` meaningful

Use `where` for:

- exact value filters (name, return_type, access, type, etc.)
- boolean flag filters (static, const, virtual, override)
- `any_of` disjunctions
- `relations` for inheritance, call, and override relationships
- `param_types` for exact parameter type matching

Do not use `where` for:
- narrative instructions or natural language descriptions
- file paths (use `scope.path` instead)
- `all_of` or `not` (silently ignored)

## Use `any_of` when the code could express the same intent in multiple ways

Examples:

- free function call vs method call
- class vs struct
- direct name vs qualified name
- multiple candidate target symbols

## Use flat keys for AND conditions

Multiple keys in a `where` object are automatically AND'd:

```json
{"access": "public", "const": true, "return_type": "std::string"}
```

This matches symbols that are public AND const AND return std::string.

Do NOT use `all_of` — it is silently ignored. Use flat keys instead.

## Prefer `count` and `exists` when possible

Do not fetch full item lists if the user only wants:

- "how many?"
- "does any exist?"

# Canonical calling patterns

## A. Resolve a function by name

```json
{
  "name": "main",
  "entity": "function"
}
```

## B. Describe one symbol

```json
{
  "symbol_id": "opaque-symbol-id"
}
```

## B2. Describe one symbol with its full source body

```json
{
  "symbol_id": "opaque-symbol-id",
  "include_source": true
}
```

Response will contain `source` and `extent` fields on the item.

## B3. List all functions with their source bodies

```json
{
  "action": "list",
  "entity": "function",
  "include_source": true
}
```

Each item in the response will contain `source` and `extent` fields.

## C. List methods in a file

```json
{
  "action": "list",
  "entity": "method",
  "scope": {
    "path": "src/widget.cpp"
  },
  "fields": [
    "symbol_id",
    "name",
    "qualified_name",
    "signature",
    "const",
    "virtual",
    "override",
    "access",
    "location"
  ]
}
```

Note: `scope.inside_class` is not yet implemented. To find methods of a specific class, list methods in the file where the class is defined and filter by `qualified_name` prefix client-side.

## D. Count virtual methods in a file

```json
{
  "action": "count",
  "entity": "method",
  "scope": {
    "path": "src/widget.cpp"
  },
  "where": {
    "virtual": true
  }
}
```

## E. Check whether a virtual destructor exists

```json
{
  "action": "exists",
  "entity": "destructor",
  "scope": {
    "path": "src/widget.cpp"
  },
  "where": {
    "virtual": true
  }
}
```

# Complex `where` queries

These examples demonstrate how to express typical real-world tasks using
the **actually implemented** backend features.

**Supported `where` composition:**
- Multiple flat keys in one `where` object are AND'd automatically
- `any_of` array — at least one sub-condition must match
- `relations` object — with `derives_from`, `overrides`, `calls`, `called_by`
- `param_types` array — exact match

**Not supported (silently ignored):**
- `all_of` — use flat keys instead
- `not` — filter client-side
- Top-level `derives_from`, `calls`, `overrides`, `called_by` outside `relations`
- `scope.inside_function`, `scope.inside_class`, `scope.in_namespace`
- Regex or pattern matching in values
- `contains`

---

## 1) Find calls to `add` in a specific file

Since `scope.inside_function` is not implemented, scope to a file instead.

Resolve `add` first, then use `relations.calls`:

```json
{
  "action": "find",
  "entity": "call",
  "scope": {
    "path": "src/main.cpp"
  },
  "where": {
    "relations": {
      "calls": "YAML::Qualified::add"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location"
  ]
}
```

If the exact symbol_id is known from `cpp_resolve_symbol`:

```json
{
  "action": "find",
  "entity": "call",
  "scope": {
    "path": "src/main.cpp"
  },
  "where": {
    "relations": {
      "calls": "c:@N@ns@F@add#I#I#"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location"
  ]
}
```

---

## 2) Find all public const methods returning `std::string` in a file

Use flat keys — they are AND'd automatically:

```json
{
  "action": "find",
  "entity": "method",
  "scope": {
    "path": "src/widget.cpp"
  },
  "where": {
    "access": "public",
    "const": true,
    "return_type": "std::string"
  },
  "fields": [
    "symbol_id",
    "name",
    "qualified_name",
    "signature",
    "return_type",
    "const",
    "access",
    "location"
  ]
}
```

---

## 3) Find all override methods in a file

```json
{
  "action": "find",
  "entity": "method",
  "scope": {
    "path": "src/derived.cpp"
  },
  "where": {
    "override": true
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "signature",
    "override",
    "location"
  ]
}
```

---

## 4) Find all classes deriving from a base class

Use the `relations` wrapper with the base class qualified name or symbol_id:

```json
{
  "action": "find",
  "entity": "class",
  "scope": {
    "path": "include/yaml-cpp/exceptions.h"
  },
  "where": {
    "relations": {
      "derives_from": "YAML::Exception"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location"
  ]
}
```

---

## 5) Find private fields in a file

Since `scope.inside_class` is not implemented, scope to file and use flat key:

```json
{
  "action": "find",
  "entity": "field",
  "scope": {
    "path": "src/config.cpp"
  },
  "where": {
    "access": "private"
  },
  "fields": [
    "symbol_id",
    "name",
    "type",
    "access",
    "location"
  ]
}
```

To further filter by type (e.g. pointers), inspect results client-side since regex is not supported.

---

## 6) Find calls in a file (multi-step for nested filters)

Nested scope filters like `inside_function` + `inside_class` are **not supported**.
Use a two-step approach:

1. List methods of interest:
```json
{
  "action": "list",
  "entity": "method",
  "scope": { "path": "src/parser.cpp" },
  "where": { "return_type": "bool" },
  "fields": ["symbol_id", "qualified_name"]
}
```

2. For each method, describe it with `include_source: true` to inspect its body.

---

## 7) Find functions matching return type and param types

Use flat keys — no `all_of` needed:

```json
{
  "action": "find",
  "entity": "function",
  "scope": {
    "path": "src/util.cpp"
  },
  "where": {
    "return_type": "int",
    "param_types": ["int", "int"]
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "signature",
    "return_type",
    "location"
  ]
}
```

Note: `param_types` requires exact type strings. Wildcard `"*"` is not supported.

---

## 8) Find static fields with a specific name

```json
{
  "action": "find",
  "entity": "field",
  "where": {
    "name": "instance",
    "static": true
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "type",
    "static",
    "location"
  ]
}
```

---

## 9) Check whether any method named `clone` exists

```json
{
  "action": "exists",
  "entity": "method",
  "where": {
    "name": "clone"
  }
}
```

To also check return type, describe matched symbols individually since regex is not supported.

---

## 10) Count calls in a file

Since `scope.inside_function` is not implemented, scope to the file:

```json
{
  "action": "count",
  "entity": "call",
  "scope": {
    "path": "src/indexer.cpp"
  },
  "where": {
    "name": "push_back"
  }
}
```

For relation-based counting, use `relations.calls` with a resolved symbol_id.

---

## 11) Find virtual or override methods using `any_of`

```json
{
  "action": "find",
  "entity": "method",
  "scope": {
    "path": "src/session.cpp"
  },
  "where": {
    "any_of": [
      { "virtual": true },
      { "override": true }
    ]
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "signature",
    "virtual",
    "override",
    "access",
    "location"
  ]
}
```

To exclude private results, filter client-side (since `not` is not implemented).

---

## 12) Find variables with a specific type in a file

```json
{
  "action": "find",
  "entity": "variable",
  "scope": {
    "path": "src/main.cpp"
  },
  "where": {
    "type": "std::string"
  },
  "fields": [
    "symbol_id",
    "name",
    "type",
    "location"
  ]
}
```

---

## 13) Find constructors with specific parameter types

```json
{
  "action": "find",
  "entity": "constructor",
  "scope": {
    "path": "src/widget.cpp"
  },
  "where": {
    "param_types": ["const std::string &"]
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "signature",
    "location"
  ]
}
```

To find constructors with any single parameter, list all constructors and filter client-side by `param_types` length.

---

## 14) Find namespaces (multi-step)

Namespace containment queries are not directly supported. Use this approach:

1. Resolve or find the target symbol
2. Inspect `qualified_name` — extract the namespace prefix
3. If needed, query `entity=namespace` to list available namespaces

---

## 15) Find classes in files matching a glob that derive from a base

Use `scope.path` with a glob pattern and `relations.derives_from`:

```json
{
  "action": "find",
  "entity": "class",
  "scope": {
    "path": "src/*.cpp"
  },
  "where": {
    "relations": {
      "derives_from": "YAML::Node"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location"
  ]
}
```

# Safe `where` composition patterns

Use these structures consistently:

## Simple exact filter

```json
{
  "name": "foo"
}
```

## Multiple conditions (AND)

Use flat keys — they are AND'd automatically:

```json
{
  "access": "public",
  "const": true,
  "return_type": "std::string"
}
```

Do NOT use `all_of` — it is silently ignored.

## Any condition may hold (OR)

```json
{
  "any_of": [
    { "name": "foo" },
    { "qualified_name": "ns::foo" }
  ]
}
```

## Relation filter

```json
{
  "relations": {
    "derives_from": "YAML::Exception"
  }
}
```

## Combining AND with OR

Flat keys + `any_of` — flat keys are AND'd, then `any_of` adds an OR clause:

```json
{
  "access": "public",
  "any_of": [
    { "virtual": true },
    { "override": true }
  ]
}
```

This matches: public AND (virtual OR override).

## Negation

`not` is **not implemented**. To exclude results, filter client-side or restructure the query using inclusive filters.

# Preferred multi-step plans for hard queries

## "Find all callers of a specific overload"

1. resolve the symbol
2. inspect candidates
3. choose one exact overload
4. run `cpp_semantic_query` with `where.relations.called_by` = resolved symbol_id
5. request only caller identity, signature, and location

## "Find all calls to X inside Y"

Since `scope.inside_function` is not implemented:

1. resolve `X`
2. scope to the file containing `Y` using `scope.path`
3. query `entity=call` with `where.relations.calls` = resolved X symbol_id
4. inspect results to identify which are inside `Y` by location

## "Find all overrides of Base::f"

1. resolve `Base::f`
2. run `cpp_semantic_query` for methods with `where.relations.overrides` = resolved symbol_id
3. request qualified names and locations

## "Find data members relevant to ownership"

1. query `entity=field` scoped to file containing the class
2. narrow by `access` if needed
3. inspect `type` field in results client-side to identify pointer-like types
4. request `type`, `access`, `location`

# Ambiguity handling

If `cpp_resolve_symbol` returns multiple candidates:

- do not silently choose one if the context is insufficient
- prefer exact `qualified_name`
- prefer matching entity kind
- prefer the candidate in the file or namespace already under discussion
- once chosen, use `symbol_id` for all downstream calls

# Fallback guidance

If the MCP server does not support a specific nested or advanced `where` form:

1. break the task into smaller semantic queries
2. resolve exact symbols first
3. run broader semantic queries
4. filter the final small result set client-side
5. do not claim the backend answered something it did not answer

# Anti-patterns

Do not:

- use grep when semantic identity matters
- request large result objects if count/exists is sufficient
- search by bare name when you already have `symbol_id`
- mix unrelated conditions into one broad query if a two-step flow is clearer
- silently collapse overload sets without acknowledging ambiguity
- use `all_of` or `not` in `where` — they are silently ignored
- put `derives_from`, `calls`, `overrides`, `called_by` as top-level `where` keys — wrap in `relations`
- use `scope.inside_class`, `scope.inside_function`, or `scope.in_namespace` — silently ignored
- assume regex support in filter values — all matching is exact
- assume every nested `where` form exists; split into stages if uncertain

# Good response behavior after tool calls

When reporting results:

- explicitly mention ambiguity if resolution was ambiguous
- clearly state when a fallback was used
- preserve exact qualified names
- preserve locations
- distinguish between "not found", "ambiguous", and "tool failed"

# Default recipes

## Recipe: explain a symbol before changing it

1. `cpp_resolve_symbol`
2. `cpp_describe_symbol`

## Recipe: edit code touching virtual dispatch

1. resolve the class or method
2. find overrides / base relations
3. describe key symbols
4. then propose edits

## Recipe: understand a function body semantically

1. resolve the function
2. describe with `"include_source": true` to get the full body
3. find calls inside it
4. find variables inside it if needed
5. describe important callee symbols

## Recipe: retrieve source of all functions in a file

1. `cpp_semantic_query` with `{"entity": "function", "action": "list", "include_source": true}`
2. each item contains the full source body and line extent
3. use `scope.path` to target a specific file or glob pattern if needed

## Recipe: get a class definition with full source

1. `cpp_resolve_symbol` for the class name
2. `cpp_describe_symbol` with `{"symbol_id": "...", "include_source": true}`
3. the response contains the class source, members, and relations

# Final preference order

When the user asks about C++ code semantics:

1. semantic MCP tool
2. exact resolved symbol
3. bounded structured result
4. only then text fallback
