# MacReleaseFileRow

One file of a release: its name in mono, then its size at the right (`name`, `size`).

- `kind`: `opens` (the default) is a document or a track sheet, a button named by its file that opens it. `audio` is a track that plays, with its Play button before the size. `other` is a file that opens nothing.
- `damaged`: why the file is damaged, after its name in the warning colour with the warning glyph. A screen reader skips the glyph, since the words after it name the damage. A damaged file opens nothing.

**In the code:** macOS: `MacReleaseFileRow` (`bae-macos/bae/bae/Views/Import/Mapping/MacReleaseFileRow.swift`).
