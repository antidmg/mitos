# Changelog

Mitos development began from Helix at commit `f9928f57f`.

The complete upstream release history remains available in the repository's
Git history and in the [Helix changelog](https://github.com/helix-editor/helix/blob/master/CHANGELOG.md).

## Unreleased

- Add `replace` on `Alt-r` to replace selections with prompted literal text in one
  undo step. `.` repeats the committed replacement.
- Rename the previous `replace` command to `replace_char`; its `r` binding keeps
  the existing character replacement behavior. Custom keymaps that use `replace`
  for character replacement should use `replace_char`.
