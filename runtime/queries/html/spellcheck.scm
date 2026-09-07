; Visible text, including headings and link labels. Tags, attributes, and
; entities are separate nodes and are not prose.
(text) @spell

; These elements contain code even when the grammar represents it as text.
((element
  (start_tag (tag_name) @_tag)) @nospell
  (#match? @_tag "(?i)^(code|pre|kbd|samp)$"))

; Also exclude any prose captures contributed by injected script/style grammars.
[
  (script_element)
  (style_element)
] @nospell
