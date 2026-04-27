# TypeScript Codemod Tooling Comparison (2026)

## 🧠 Overview

There is no single “best” codemod toolkit. The ecosystem is evolving toward a combination of:

* **Pattern-based AST tools** (fast, declarative)
* **Type-aware tools** (semantic, safe refactors)

---

# 🥇 Modern Tooling Landscape

## 1. ast-grep (and jssg)

**Category:** Pattern-based AST transformations
**Language:** Multi-language (JS, TS, HTML, CSS, etc.)
**Engine:** Rust (fast)

### Pros

* Very fast
* Declarative (YAML or simple patterns)
* Works across multiple languages
* Great for large-scale migrations
* Modern developer experience

### Cons

* Less expressive for complex logic
* Limited type awareness

### Notes

* `jssg` builds on ast-grep and aims to replace jscodeshift

---

## 2. ts-morph

**Category:** TypeScript compiler wrapper
**Language:** TypeScript / JavaScript
**Engine:** TypeScript compiler API

### Pros

* Full type awareness
* Safe symbol renaming
* Can analyze references across project
* Ideal for semantic refactors

### Cons

* Slower (loads full TS program)
* Formatting is not preserved as cleanly
* More verbose API

---

## 3. jscodeshift

**Category:** AST transformation toolkit
**Language:** JavaScript / TypeScript
**Engine:** Babel + recast

### Pros

* Mature and widely used
* Good formatting preservation
* Large ecosystem of existing codemods
* Parallel execution support

### Cons

* Syntax-only (no type awareness)
* Aging API design
* Slower innovation / maintenance

---

## 4. Lower-Level Building Blocks

### recast

* Preserves formatting when printing AST

### Babel / TypeScript AST APIs

* Maximum control
* Used internally by most tools

---

# 🧭 When to Use What

## Use ast-grep / jssg when:

* You want fast bulk transformations
* Patterns are simple or structural
* You need cross-language changes
* You prefer declarative rules

## Use ts-morph when:

* You need type-aware refactoring
* You’re renaming symbols safely
* You depend on type inference

## Use jscodeshift when:

* You want a proven ecosystem
* Formatting preservation is critical
* You’re using existing codemods

---

# 🧩 Generational Model

| Generation | Tooling     | Approach                   |
| ---------- | ----------- | -------------------------- |
| Gen 1      | jscodeshift | Imperative AST transforms  |
| Gen 2      | ts-morph    | Type-aware transformations |
| Gen 3      | ast-grep    | Declarative + fast         |

---

# 👍 Recommended Stack

### Most modern approach:

```text
ast-grep (or jssg) + ts-morph
```

### Most practical today:

```text
jscodeshift
```

---

# 🏁 Summary

| Tool        | Strength          | Weakness           |
| ----------- | ----------------- | ------------------ |
| ast-grep    | Fast, declarative | Limited semantics  |
| ts-morph    | Type-aware        | Slower, formatting |
| jscodeshift | Mature, stable    | Aging, syntax-only |

---

If you want, I can also turn this into a README with examples or a starter repo setup.
