# Adding Spellcheck Queries

Mitos uses `spellcheck.scm` tree-sitter query files to decide which parts of
a document are spell checked. See [Spell checking](../spell-checking.md) for
info about the feature.

Tree-sitter queries are documented in the tree-sitter online documentation. If
you're writing queries for the first time, be sure to check out the section on
[syntax highlighting queries] and on [query syntax].

Spellcheck queries have two captures:

- `@spell` marks a node whose text should be checked.
- `@nospell` excludes a node (or part of one) from a surrounding `@spell`
  capture.

Queries go in a `runtime/queries/<language>/spellcheck.scm` file. A language
with no query has no checked regions of its own; injected languages can still
provide queries. Files without a syntax tree, including plain text, are checked
in full.

## Checking comments

Most languages inject a shared `comment` grammar into their comments, and that
grammar has a query (`runtime/queries/comment/spellcheck.scm`) capturing
comment text. So you usually do not need a query just to check a language's
comments.

## Choosing prose regions

Prefer nodes that represent prose over capturing an entire document or every
string. For example, Python's query selects the first string statement in a
module, class, or function; JSX's query selects rendered text while leaving
expressions and property values alone.

Use `@nospell` on enclosing code regions when their children can otherwise
match `@spell`. HTML's `pre` and `code` elements are examples. Exclusions also
apply to captures from injected languages, so a code block can exclude comments
that an injected grammar would otherwise check.

Add coverage cases to `crates/core/tests/spellcheck.rs` for both prose and
nearby code, metadata, and escapes. Validate the query against the configured
grammar with `cargo xtask query-check <language>`, then run
`cargo test -p core --test spellcheck`. Query compilation alone cannot show
whether the intended text is selected.

## An example

To check the contents of strings in a language, capture the string's text node:

```scm
(string_content) @spell
```

Use `@nospell` to carve out parts that aren't prose. For example, to check a
string but skip an interpolation or escape inside it:

```scm
(string (string_content) @spell)
(escape_sequence) @nospell
```

The `:tree-sitter-subtree` command shows the syntax tree under the primary
selection and is the easiest way to find the node names to capture.

[syntax highlighting queries]: https://tree-sitter.github.io/tree-sitter/syntax-highlighting#highlights
[query syntax]: https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
