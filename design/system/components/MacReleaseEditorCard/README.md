# MacReleaseEditorCard

A library release's editor card, in the album's Edit release sheet and on an imported folder's pane: Identify with Automatic and Search, then Cancel once something changed (`changed`) and Save, after a spinner and `reading` while a save reads a file before it writes; the cover and the identity; the Release grid; the records under a rule, with no Unlink, since nothing unlinks a library release.

It takes the same values as `MacImportDraft`: `cover`, `title`, `artist`, `artistChip`, `year`, `audio`, `album` (with `albumInLibrary`, `albumPicked` and `albumOpen`), the Release grid's `pressingYear`, `media`, `labelName`, `catalog`, `country`, `barcode`, `status`, `packaging` and `details`, and `records`.

**In the code:** macOS: `MacReleaseEditorCard` (`Views/Components/MetadataForm/MacReleaseEditorCard.swift`), which `ReleaseMetadataEditorContent` places.
