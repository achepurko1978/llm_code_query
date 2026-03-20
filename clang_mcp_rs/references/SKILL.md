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

Use this when the user asks for things "inside" a function or class.

Examples:

- calls inside `main`
- declarations inside `parse_config`
- fields inside `Widget`
- methods inside `Base`

Preferred flow:

1. resolve the scope symbol if needed
2. pass the scope in the `scope` parameter
3. add `where` filters only for the specific thing being searched

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

Common relations include:

- `derives_from`
- `overrides`
- `calls`
- `called_by`
- `inside_function`
- `inside_class`
- `in_namespace`

## Filters

Common filter fields include:

- `name`
- `qualified_name`
- `return_type`
- `type`
- `param_types`
- `static`
- `const`
- `virtual`
- `override`
- `access`

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

- exact filters
- boolean combinations
- relation disjunctions
- narrowing by access, qualifiers, types, names, files

Do not use `where` for narrative instructions or natural language descriptions.

## Use `any_of` when the code could express the same intent in multiple ways

Examples:

- free function call vs method call
- class vs struct
- direct name vs qualified name
- multiple candidate target symbols

## Use `all_of` when every condition must hold

Examples:

- method AND const AND public
- field AND static AND private
- call AND inside function AND calls exact symbol

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

## C. List methods of a class

```json
{
  "action": "list",
  "entity": "method",
  "scope": {
    "inside_class": "MyClass"
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

## D. Count virtual methods in a class

```json
{
  "action": "count",
  "entity": "method",
  "scope": {
    "inside_class": "MyClass"
  },
  "where": {
    "virtual": true
  }
}
```

## E. Check whether a class has a virtual destructor

```json
{
  "action": "exists",
  "entity": "method",
  "scope": {
    "inside_class": "MyClass"
  },
  "where": {
    "all_of": [
      { "name": "~MyClass" },
      { "virtual": true }
    ]
  }
}
```

# Complex `where` queries

These examples demonstrate how to express typical real-world tasks.

Note: the exact accepted shape depends on the MCP schema implemented by the server. Use the patterns below and keep the structure regular.

---

## 1) Find every call to `add` inside `main`

If possible, resolve `main` first, and resolve `add` first.

Then query:

```json
{
  "action": "find",
  "entity": "call",
  "scope": {
    "inside_function": "main"
  },
  "where": {
    "any_of": [
      {
        "calls": {
          "name": "add",
          "entity": "function"
        }
      },
      {
        "calls": {
          "name": "add",
          "entity": "method"
        }
      }
    ]
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location",
    "source_excerpt"
  ]
}
```

If an exact resolved target is available, prefer:

```json
{
  "action": "find",
  "entity": "call",
  "scope": {
    "inside_function": "main"
  },
  "where": {
    "calls": {
      "symbol_id": "resolved-add-symbol-id"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location",
    "source_excerpt"
  ]
}
```

---

## 2) Find all public const methods of `Widget` returning `std::string`

```json
{
  "action": "find",
  "entity": "method",
  "scope": {
    "inside_class": "Widget"
  },
  "where": {
    "all_of": [
      { "access": "public" },
      { "const": true },
      { "return_type": "std::string" }
    ]
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

## 3) Find all methods of `Derived` that override something

```json
{
  "action": "find",
  "entity": "method",
  "scope": {
    "inside_class": "Derived"
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

If relation form is supported more robustly than the boolean flag, prefer:

```json
{
  "action": "find",
  "entity": "method",
  "scope": {
    "inside_class": "Derived"
  },
  "where": {
    "overrides": {
      "exists": true
    }
  }
}
```

---

## 4) Find all classes deriving from `Base`

```json
{
  "action": "find",
  "entity": "class",
  "where": {
    "derives_from": {
      "name": "Base"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location"
  ]
}
```

Prefer exact symbol target if resolved:

```json
{
  "action": "find",
  "entity": "class",
  "where": {
    "derives_from": {
      "symbol_id": "resolved-base-symbol-id"
    }
  }
}
```

---

## 5) Find fields in `Config` that are private and have pointer-like types

```json
{
  "action": "find",
  "entity": "field",
  "scope": {
    "inside_class": "Config"
  },
  "where": {
    "all_of": [
      { "access": "private" },
      {
        "any_of": [
          { "type": ".*\\*" },
          { "type": "std::unique_ptr<.*>" },
          { "type": "std::shared_ptr<.*>" }
        ]
      }
    ]
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

Use this only if the backend supports pattern-like type matching. If not, split into separate exact queries.

---

## 6) Find calls made inside methods of `Parser` that return `bool`

```json
{
  "action": "find",
  "entity": "call",
  "scope": {
    "inside_class": "Parser"
  },
  "where": {
    "inside_function": {
      "all_of": [
        { "entity": "method" },
        { "return_type": "bool" }
      ]
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location",
    "source_excerpt"
  ]
}
```

If nested scope filters are not supported, do it in two steps:

1. find matching methods of `Parser`
2. query calls inside each method

---

## 7) Find free functions in namespace `util` that take two parameters and return `int`

```json
{
  "action": "find",
  "entity": "function",
  "scope": {
    "in_namespace": "util"
  },
  "where": {
    "all_of": [
      { "return_type": "int" },
      { "param_types": ["*", "*"] }
    ]
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

If wildcard parameter matching is not supported, prefer exact `param_types` or use `cpp_describe_symbol` after a broader list query.

---

## 8) Find static fields named `instance` in any class

```json
{
  "action": "find",
  "entity": "field",
  "where": {
    "all_of": [
      { "name": "instance" },
      { "static": true }
    ]
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

## 9) Check whether any method named `clone` returns a smart pointer

```json
{
  "action": "exists",
  "entity": "method",
  "where": {
    "all_of": [
      { "name": "clone" },
      {
        "any_of": [
          { "return_type": "std::unique_ptr<.*>" },
          { "return_type": "std::shared_ptr<.*>" }
        ]
      }
    ]
  }
}
```

Use pattern-like matching only if the server supports it.

---

## 10) Count calls to `push_back` inside `build_index`

```json
{
  "action": "count",
  "entity": "call",
  "scope": {
    "inside_function": "build_index"
  },
  "where": {
    "calls": {
      "name": "push_back"
    }
  }
}
```

---

## 11) Find methods inside `Session` that are either virtual or override, but not private

```json
{
  "action": "find",
  "entity": "method",
  "scope": {
    "inside_class": "Session"
  },
  "where": {
    "all_of": [
      {
        "any_of": [
          { "virtual": true },
          { "override": true }
        ]
      },
      {
        "not": {
          "access": "private"
        }
      }
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

Use `not` only if the server supports it well. Otherwise query for public/protected explicitly.

---

## 12) Find variables inside `main` whose type is `std::string`

```json
{
  "action": "find",
  "entity": "variable",
  "scope": {
    "inside_function": "main"
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

## 13) Find constructors of `Widget` that take exactly one parameter

```json
{
  "action": "find",
  "entity": "constructor",
  "scope": {
    "inside_class": "Widget"
  },
  "where": {
    "param_types": ["*"]
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "signature",
    "location"
  ]
}
```

If wildcard parameter lists are unsupported, first list constructors, then filter client-side.

---

## 14) Find all namespaces containing a symbol named `parse`

```json
{
  "action": "find",
  "entity": "namespace",
  "where": {
    "contains": {
      "name": "parse"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location"
  ]
}
```

Use only if containment relations are supported. Otherwise:
1. resolve or find `parse`
2. inspect `qualified_name`
3. derive the namespace from the result

---

## 15) Find classes in files matching `parser` that derive from `Node`

```json
{
  "action": "find",
  "entity": "class",
  "scope": {
    "path": "**/parser*"
  },
  "where": {
    "derives_from": {
      "name": "Node"
    }
  },
  "fields": [
    "symbol_id",
    "qualified_name",
    "location"
  ]
}
```

Use `scope.path` with a glob pattern to constrain by file.

# Safe boolean composition patterns

Use these structures consistently:

## Simple exact filter

```json
{
  "name": "foo"
}
```

## All conditions must hold

```json
{
  "all_of": [
    { "entity": "method" },
    { "const": true },
    { "access": "public" }
  ]
}
```

## Any condition may hold

```json
{
  "any_of": [
    { "name": "foo" },
    { "qualified_name": "ns::foo" }
  ]
}
```

## Negation

```json
{
  "not": {
    "access": "private"
  }
}
```

Use negation only if the server supports it well.

# Preferred multi-step plans for hard queries

## "Find all callers of a specific overload"

1. resolve the symbol
2. inspect candidates
3. choose one exact overload
4. run `cpp_semantic_query` with `called_by`
5. request only caller identity, signature, and location

## "Find all calls to X inside Y"

1. resolve `Y`
2. resolve `X`
3. query `entity=call`
4. constrain with `scope.inside_function=Y`
5. constrain with `where.calls = X`

## "Find all overrides of Base::f"

1. resolve `Base::f`
2. run `cpp_semantic_query` for methods with `overrides = resolved symbol`
3. request qualified names and locations

## "Find data members relevant to ownership"

1. query `entity=field`
2. scope to class if available
3. narrow by `type`
4. use `any_of` across raw pointer / unique_ptr / shared_ptr patterns
5. request `type`, `access`, `location`

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
- assume regex support unless confirmed by schema
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
