; A docstring is the first statement of a module, class, or function. Comments
; may precede it. Ordinary strings, bytes, and f-strings are not documentation.
(module
  . (comment)*
  . (expression_statement
      .
      (string
        (string_start) @_start
        (string_content) @spell)
      .)
  (#not-match? @_start "[bBfF]"))

([
  (class_definition
    body: (block
      . (comment)*
      . (expression_statement
          .
          (string
            (string_start) @_start
            (string_content) @spell)
          .)))
  (function_definition
    body: (block
      . (comment)*
      . (expression_statement
          .
          (string
            (string_start) @_start
            (string_content) @spell)
          .)))
]
  (#not-match? @_start "[bBfF]"))

(escape_sequence) @nospell
