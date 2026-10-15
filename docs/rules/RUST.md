# Rust Code Guidelines

Guidelines for writing and reviewing Rust code and Cargo manifests in this repository.
Use the official default style and let `rustfmt` handle mechanical formatting.
Spend review time on correctness and clarity, not hand-adjusted whitespace.

The [official Rust Style Guide](https://doc.rust-lang.org/style-guide/index.html)
provides context and is the fallback for style details not covered here. It primarily
covers style; it is not a complete guide to API design, ownership, error handling,
security, or testing. Repository-specific requirements are labeled separately.

## RUST-1: Formatting foundations

### Whitespace and layout

- Use spaces, never tabs; indent by **4 spaces** per level.
- Use a **100-character maximum code line width**, subject to the guide's explicit
  exceptions and the formatter's handling of constructs such as literals.
- Prefer block indentation over aligning continuation lines with an earlier token.
- Separate items and statements with zero or one blank line, not runs of blank lines.
- Do not leave trailing whitespace, including in comments and blank lines. When
  changing string literals, preserve their actual values.
- Use trailing commas in newline-terminated comma-separated lists. Syntax-specific
  exceptions matter: for example, no comma after a struct update's `..base`.
- Let `rustfmt` decide whether a construct is small enough for single-line formatting;
  the guide deliberately leaves the exact size heuristics to tools.

```rust
let session = create_session(
    remote_host,
    worktree_path,
    session_options,
);
```

### Sorting

Where sorting is prescribed, use **version sorting**, not simple lexical sorting:
`item8` comes before `item16`. Underscores sort before other non-space characters;
non-lowercase characters sort before lowercase characters. Leave detailed Unicode
and leading-zero tie-breaking to the formatter.

Sorting rules do not authorize reordering arbitrary declarations, fields, variants,
or operations. Preserve semantics and deliberate logical groupings.

### Comments and documentation

- Prefer `//` over block comments; put one space after the comment marker.
- Prefer comments on their own lines. An end-of-line comment has one space before `//`.
- Usually write complete sentences, capitalized and ending with a period.
- Limit comment-only lines to 80 characters excluding indentation, or 100 including
  indentation, whichever permits the shorter line.
- For a single-line block comment, use `/* explanation */`. For multiline block
  comments, put the opening and closing delimiters on separate lines.
- Prefer `///` for item documentation. Reserve `//!` for crate or module documentation.
- Put doc comments **before attributes**.
- Review comments manually: formatters need not enforce the comment recommendations.

### Attributes

- Put each attribute on its own line at the indentation level of its item.
- Prefer outer attributes; indent inner attributes to the inside of their item.
- Format attribute argument lists like function arguments; use spaces around `=`.
- Use a single `#[derive(...)]` attribute. Preserve derive order when consolidating
  attributes, because order can affect correctness.

```rust
/// Identifies a remote session.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionId(String);
```

## RUST-2: Items and module organization

These rules also apply to items declared inside functions or other items.

### Imports and modules

- If present, place `extern crate` declarations first, alphabetically ordered.
  This is a placement rule, not a requirement to introduce them.
- Place `use` declarations before out-of-line module declarations (`mod name;`),
  and both before other items.
- Version-sort imports within each contiguous group and sort module declarations.
- Preserve import groups separated by blank lines or other items. Attributes start
  a new group and prevent reordering across that boundary.
- Do not automatically move `#[macro_use]` module declarations; order may matter.
- Within import lists, put `self` and `super` first, then version-sorted names;
  nested groups and globs come last at their respective level.
- Keep imports on one line when possible, without spaces inside braces.
- Prefer several manageable imports over a large multiline import. Do not perform
  unrelated merging, splitting, or glob changes as part of formatting.
- For multiline lists, break after `{` and before `}`, block-indent, and use a
  trailing comma. Nested import lists require multiline formatting, with each nested
  import on its own line.
- Simplify `use a::self;` to `use a;`, `use a::{b};` to `use a::b;`, and remove
  empty imports such as `use a::{};`.

```rust
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod client;
mod config;
```

The grouping above is illustrative; the official guide does not mandate separate
standard-library, third-party, and local groups.

### Functions

- Keep `fn function_name` together so declarations remain easy to search.
- Avoid comments inside function signatures.
- Keep signatures on one line when they fit. Otherwise put each argument on its
  own block-indented line, with a trailing comma and a separate closing parenthesis.
- Use `name: Type`, comma-space separators, and spaces around `->`.
- Normally keep the opening body brace on the signature line; multiline `where`
  clauses have their own layout below.

```rust
fn session_label(host: &str, name: &str) -> String {
    format!("{host}:{name}")
}
```

### Structs, unions, tuples, and enums

- Put named struct and union fields on separate indented lines with trailing commas.
- Prefer unit structs (`struct Marker;`) over empty tuple or braced structs.
- Keep tuple structs on one line when short. For more than a few fields, especially
  when the declaration becomes multiline, prefer named fields.
- Put each enum variant on its own indented line with a trailing comma.
- A small struct-like variant may stay on one line. If one struct-like variant is
  multiline, use multiline formatting for all struct-like variants in that enum;
  consider extracting substantial variant data into separate structs.

```rust
struct Session {
    host: String,
    name: String,
}

enum SessionState {
    Starting,
    Running { pid: u32 },
    Failed { message: String },
}
```

### Traits, implementations, generics, and bounds

- Block-indent trait and implementation contents; empty bodies may use `{}` inline.
- Keep implementation headers together when possible. If a trait implementation
  header must split, break before `for` and put its opening brace on a separate line.
- Write bounds as `T: Display + Debug`, with a space after `:` and around `+`.
- Keep generic parameter lists inline when practical; move lengthy bounds to `where`.
- Prefer single-letter generic parameter names, as recommended by the guide.
- Do not put padding inside `<...>`; separate parameters with comma-space.
- Use spaces around associated-type equality: `Iterator<Item = String>`.
- For multiline generics, put one parameter per indented line and use trailing commas.
- In multiline `where` clauses, put each predicate on its own indented line, with a
  trailing comma unless the clause terminates with a semicolon. Put the following
  body brace on a new line.
- `where` follows a closing bracket on the same line; otherwise it starts a new line
  at the item's indentation. Very short bounds are better written inline.
- If a bound must wrap, break before each `+` and indent the continuation.

```rust
fn collect_names<I>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    names.into_iter().collect()
}
```

### Aliases, foreign items, and macro definitions

- Keep type aliases on one line when possible. If wrapping is necessary, normally
  break before `=` and indent the right-hand side. Associated types use the same rules.
- Consult the source for aliases with preceding or trailing `where` clauses;
  ordinary type-alias `where` bounds are not enforced and should not be used.
- Explicitly name the ABI for foreign items: use `extern "C" fn`, not `extern fn`.
  Under Rust 2024, foreign blocks use `unsafe extern "C" { ... }`.
- Use braces around the full definition of a `macro_rules!` macro.

## RUST-3: Statements

- Write bindings as `let pattern: Type = expression;`, with no space before `;`.
- Prefer single-line bindings. When wrapping, first try breaking after `=` and
  block-indenting the value; split after `:` if the declaration still does not fit.
- A multiline initializer may start immediately after `=` when its first line fits;
  do not add an unnecessary extra indentation level to the entire initializer.
- Format the binding portion of `let ... else` like an ordinary binding.
- Keep `else {` together. A short let-else may fit on one line only when its else
  block contains a single-line expression, no statements, and no comments.
- Otherwise use a multiline else block. For a multiline initializer, `else {` may
  join its closing-delimiter line only when that line contains only closing
  delimiters and aligns with `let`; otherwise place `else {` on its own line.
- Terminate expression statements with semicolons, except block-ending expressions
  and expressions intentionally supplying a block's value.
- Use a semicolon for a unit-returning call used only for its side effect, even if
  its unit value could be propagated.
- Statement macro invocations use parentheses or square brackets and end in `;`,
  without spaces around the name, `!`, delimiters, or semicolon.

```rust
fn print_name(name: Option<&str>) {
    let Some(name) = name else {
        return;
    };

    println!("{name}");
}
```

## RUST-4: Expressions

### Blocks and closures

- Normally break after a block's `{` and before its `}`; indent its contents.
- Keep block keywords such as `async` and `unsafe` next to the opening brace.
- Write empty blocks as `{}`. Avoid comments on brace lines.
- A block with one single-line expression, no statements, and no comments may be
  inline in expression position, or for an unsafe block in statement position.
- Prefer expression closures such as `|name| name.len()` without unnecessary braces
  or parameter type annotations.
- Add closure braces for explicit return types, statements, comments, or multiline
  control-flow bodies. Write `move |value| ...` with a space after `move`.

### Literals and construction

- Small struct literals use `Session { host, name }`; larger ones use one field per
  line and trailing commas. Use a space after field colons, not before.
- Struct update syntax is `..base`, with **no trailing comma** after it.
- Format tuples and tuple-struct literals like argument lists. Keep the required
  comma in a one-element tuple: `(value,)`.
- Prefer qualified enum variants (`SessionState::Starting`) except for prelude
  variants such as `Some`, `None`, `Ok`, and `Err`.
- Use `[a, b, c]`, `[value; count]`, and `vec![a, b, c]`; array-like macros use `[]`.
- Multiline arrays are block-indented. For a repeating initializer, break after `;`.
- Use consistent hexadecimal letter case throughout the project; never mix upper
  and lower case within a single hexadecimal literal. The guide chooses neither case.
- Keep the unit literal `()` together; see the style-edition caveat in RUST-9.

### Operators, ranges, and indexing

- Put spaces around binary and assignment operators: `a + b`, `count += 1`.
- Do not separate unary operators from their operands: `!ready`, `*value`, `&value`.
  Use a space after `&mut`.
- When wrapping, break **after assignment operators**, but **before other binary
  operators**, and block-indent continuations. Prefer the assignment break first.
- Format `as` with surrounding spaces; when wrapping, break before `as`.
- Use parentheses when they clarify precedence, not creative whitespace.
- In comparisons, prefer dereferencing an existing reference over borrowing the
  other operand when possible without unnecessary cost or changing behavior.
- Keep ranges unspaced: `0..count`, `start..=end`, `..limit`. Parenthesize compound
  bounds, for example `0..(count - 1)`. If a bounded range wraps, break before `..`.
- Write indexing without padding: `items[index]`, `&items[..limit]`. Never break
  between the indexed expression and `[`. Indent a multiline index inside brackets.

### Calls, macros, and method chains

- Write `function(a, b)` and `value.method(a)` without spaces before `(` or around `.`.
- Single-line calls have no trailing comma. Multiline calls normally have one
  argument per indented line and a trailing comma.
- Keep no-argument calls as `function()`; see RUST-9 for edition behavior.
- Format parsable macro arguments like their corresponding Rust constructs. The
  guide's macro-specific rules do not define arbitrary third-party macro syntax.
- For multiline formatting macros, small arguments may share a line before or after
  a format string on its own line; otherwise use one argument per line.
- Keep short method chains inline. Split longer chains **before `.`**, keeping `?`
  attached to the preceding expression and indenting subsequent chain elements.
- Prefer a multiline chain whose individual calls fit on single lines to one long
  chain with a heavily wrapped final call.
- If a chain element is multiline, put subsequent elements on separate lines too.
- Avoid unnecessary nesting of delimiters: a call whose only argument is a multiline
  expression may combine with that expression, as in `Some(Session { ... })`.
  A final block closure may similarly combine when earlier arguments and the closure
  opening fit and no other arguments are closures. Let `rustfmt` handle exceptions.

```rust
let names: Vec<_> = sessions
    .iter()
    .map(|session| session.name.as_str())
    .collect();

let session = Some(Session {
    host: String::from("build-host"),
    name: String::from("review"),
});
```

### Control flow and matching

- Do not wrap entire `if` or `while` conditions in unnecessary parentheses.
- Keep the keyword, condition, and opening brace together when they fit.
- Write `} else {` and `} else if condition {` on one line.
- When the control line wraps, normally put the opening brace on its own line.
  A closing-delimiter-only line aligned with the control keyword may share the brace.
- When needed, prefer wrapping after `=` in a let condition and before `in` in a
  `for` header. Block-indent continuations.
- A small `if ... else` may be inline when used as a value, not as a standalone
  statement. Prefer the form that makes the branches easy to read.
- Always put match arms on separate indented lines inside the match braces.
- Use a trailing comma for a non-block match arm, and omit it for a block arm.
- A simple arm is `pattern => expression,`. Use a block for multiple statements,
  line comments, control flow, or a body that cannot start on the pattern's line.
- Never break immediately after `=>` without introducing a block body.
- Prefer adding a block to a long arm over splitting its pattern unnecessarily.
- Do not start an arm pattern with `|`. When alternatives wrap, put `|` at the
  beginning of continuation lines, without an extra pattern indentation level.
- If a guard must wrap, break before `if`, indent the guard, and use a block body.
- Do not remove a block around a single macro call blindly: expansion may contain
  a trailing semicolon and change the arm's meaning.
- Format patterns like their corresponding expressions, subject to match-arm rules.

```rust
match name {
    Some("") | None => String::from("default"),
    Some(name) => {
        let trimmed = name.trim();
        trimmed.to_owned()
    }
}
```

The live guide also specifies let-chain layout. Let-chains are available in this
project's Rust 1.88 minimum with edition 2024. Their detailed formatting remains
in the official guide.

## RUST-5: Types and bounds

Use these forms without extra padding:

| Construct | Form |
| --- | --- |
| Slice and array | `[T]`, `[T; count]` |
| References | `&T`, `&mut T`, `&'a T`, `&'a mut T` |
| Raw pointers | `*const T`, `*mut T` |
| Tuples | `(A, B)`, `(A,)`, `()` |
| Generic types | `Result<T, E>` |
| Associated paths | `<T as Trait>::Item` |
| Function pointers | `fn(T, U) -> V`, `unsafe extern "C" fn(T) -> U` |
| Bounds | `T: Clone + Debug`, `impl Clone + Debug` |
| Precise capturing bounds | `impl Trait + use<'a, T>` |

- Avoid breaking types where possible. If necessary, break at the outermost generic
  nesting rather than deeply inside a nested argument.
- Format function types like function declarations and generic types like generic
  parameter lists.
- Break array types after `;` when needed.
- When wrapping `+` bounds, break before every `+` and indent the continuation.
- Format a precise capturing bound `use<...>` like a generic path segment; this is
  type-bound syntax, not an import declaration.

## RUST-6: Non-formatting conventions

### Prefer expression-oriented code

Use Rust expressions to compute values directly instead of declaring a variable
and assigning it independently in each branch.

```rust
let label = if connected { "online" } else { "offline" };
```

For larger branches, the same principle applies with multiline `if` or `match`
expressions; do not compress complex logic merely to save lines.

### Naming

| Entity | Convention | Example |
| --- | --- | --- |
| Types and traits | `UpperCamelCase` | `RemoteSession` |
| Enum variants | `UpperCamelCase` | `ConnectionLost` |
| Struct fields | `snake_case` | `remote_host` |
| Functions and methods | `snake_case` | `create_session` |
| Local variables | `snake_case` | `session_id` |
| Macros | `snake_case` | `session_trace!` |
| Constants and immutable statics | `SCREAMING_SNAKE_CASE` | `DEFAULT_TIMEOUT` |

For reserved words, use a raw identifier (`r#type`) or trailing underscore (`type_`),
not a deliberate misspelling. Avoid `#[path]` module annotations where possible.

## RUST-7: Cargo.toml conventions

- Use the same 100-character line width and 4-space indentation as Rust code.
- Start keys at the beginning of the line; use one space on each side of `=`.
- Put one blank line between sections, but none between a section header and its
  keys or between key-value pairs within a section.
- Put `[package]` first. Within it, put `name`, then `version`, then the remaining
  keys in version-sorted order, except `description`, which goes last.
- Version-sort keys in other sections, including dependency names.
- Use bare standard keys; quote only keys that require quoting.
- Keep short arrays inline. For multiline arrays, put one indented element per
  line, with a trailing comma, and an unindented closing bracket.
- Keep short dependency tables inline. If a table does not fit, use a separate
  section such as `[dependencies.some_crate]`, not a wrapped inline table.
- Use multiline strings for multiline values, not embedded newline escapes.
- Wrap description text at 80 columns. Describe the crate directly rather than
  starting with its name. For multiple sentences, put the summary sentence on its
  own line before further detail.
- If present, use a valid SPDX license expression and a fully qualified homepage URL.
- If present, author entries should be `Full Name <email@example.org>`; a mailing-list
  address may appear alone. These rules do not require adding optional metadata.

```toml
[dependencies]
clap = { features = ["derive"], version = "4" }
serde = { features = ["derive"], version = "1" }
serde_json = "1"
```

`cargo fmt` does not format `Cargo.toml`; review manifest formatting separately.

## RUST-8: Principles for style decisions

Apply the guide's priorities in this order:

1. **Readability:** easy scanning, no misleading layout, accessibility, and code
   that remains understandable without syntax highlighting or IDE assistance.
2. **Aesthetics:** a consistent appearance familiar to readers of other code.
3. **Practical layout:** small and merge-friendly diffs, limited rightward drift,
   and reasonable use of vertical space.
4. **Simple application:** rules that humans, formatters, editors, and generators
   can apply consistently.

Readability outranks compactness. Consider how a change looks in a plain-text diff,
compiler diagnostic, or search result, not just in your editor.

## RUST-9: Style editions and toolchain compatibility

- A **language edition** controls language semantics; a **style edition** controls
  formatting conventions. They can be migrated separately.
- Formatting defaults normally follow the crate's language edition. Older language
  editions do not necessarily imply distinct styles: 2015, 2018, and 2021 share one.
- The live guide evolves. Its newest rules and examples are not automatically
  available in every compiler or formatter supporting an older edition.
- The source lists never splitting nullary calls or unit literals under its
  “Rust next style edition” heading. Do not force a formatter upgrade or manually
  fight the repository formatter to adopt a future-edition detail.
- The retrieved page still calls Rust 2024 nightly-only. That sentence is outdated:
  Rust 2024 is stable in Rust 1.85. It is not a reason to switch this project to nightly.

**Repository policy:** `Cargo.toml` declares `edition = "2024"` and
`rust-version = "1.88"`. `mise.toml` and CI pin Rust `1.88.0` for local
development and Linux/macOS verification. Keep these pins aligned when explicitly
upgrading the toolchain. This repository has no Nix flake. Use `cargo fmt` so
formatting is aware of the Cargo edition. Retain the default style rather than
adding personal formatting overrides. Resolve discrepancies with the repository
formatter and the edition-appropriate source, not repeated manual edits. Treat
minimum-version and style-edition upgrades as explicit changes.

## RUST-10: Nightly-only syntax

This chapter has no formatting-stability guarantee. At review time it documents
`frontmatter`: manifest-like content before comments and attributes, fenced by
at least three dashes. Fences must exceed the longest leading dash sequence in the
content by one; an optional infostring is separated by one space. There are no
blank lines before frontmatter or between it and a preceding shebang, and zero or
one blank line may separate it from following content. Fence lines have no trailing
whitespace.

**Repository policy:** do not introduce nightly-only syntax or feature gates into
this stable Rust project. This chapter is mapped for completeness, not permission
to use its syntax. Revisit the relevant stable chapter when a feature stabilizes,
and check the project's minimum compiler before adopting it.

## RUST-11: Repository engineering rules and review checklist

These are project requirements, **not claims from the official Style Guide**:

- Keep changes focused and follow the existing module organization.
- Handle recoverable failures explicitly rather than panicking.
- Add dependencies only when their benefit justifies the cost.
- Document public interfaces and non-obvious behavior; update user-facing
  documentation when behavior changes.
- Add or update tests for changed behavior and preserve Rust 1.88 compatibility.
- Avoid unrelated formatting churn in functional changes.

Before submitting Rust changes, use the configured Mise toolchain:

```sh
mise exec -- cargo fmt
mise exec -- cargo fmt --check
mise exec -- cargo clippy --locked --all-targets -- -D warnings
mise exec -- cargo test --locked
```

CI uses the same checks with pinned Rust 1.88.0. If testing with a newer toolchain,
also verify the pinned version; newer-toolchain results do not establish MSRV.

Review manually what these commands do not establish:

- [ ] Names, comments, documentation, and manifest layout follow this guide.
- [ ] Imports, attributes, and macro-sensitive declarations preserve semantics.
- [ ] New syntax and dependencies remain compatible with the minimum Rust version.
- [ ] Tests cover changed behavior, including relevant error paths.
- [ ] The diff is focused and readable without editor assistance.

Formatting and Clippy are aids, not proof of correctness or minimum-version
compatibility. Validate minimum-version support with the corresponding toolchain
when changing syntax, standard-library usage, or dependencies.
