; JSX text is prose; expressions, property values, and component names are code.
(jsx_text) @spell

((jsx_element
  open_tag: (jsx_opening_element name: (identifier) @_tag)) @nospell
  (#any-of? @_tag "code" "pre" "kbd" "samp" "script" "style"))
