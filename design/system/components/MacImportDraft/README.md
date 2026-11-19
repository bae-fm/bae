# MacImportDraft

The import draft card: Identify with Automatic, Search and the ⋯ button that opens the Metadata menu; where the import goes and Import (or Retry import); the folder's `check` in a warning `MacNotice` under that row; the cover and the identity; the Release grid; the linked records under a rule, with Unlink.

- **Cover:** `cover`, or a plain tile when the draft has none. The tile says nothing and invites nothing.
- **Identity:** a `MacReleaseIdentity`: the album title (`title`), the album artist (`artist`, with `artistChip`) and the album's year (`year`). Under them the audio line (`audio`, as the scan read the folder: `FLAC · 44.1 kHz · 16-bit · Stereo`), then a `MacAlbumPicker` showing the album the release files under (`album`, with `albumInLibrary`, `albumPicked` and `albumOpen`).
- **Release grid:** a `MacReleaseFields`: Year (`pressingYear`), Media (`media`), Label (`labelName` and `catalog`), Country (`country`), Barcode (`barcode`), Status (`status`), Packaging (`packaging`) and Details (`details`).
- **Records:** a `MacRecordLinks` (`records`), then Unlink, a button that stops treating the folder as that release and leaves the metadata as it is.

A part with no value draws nothing: no audio line without `audio`, no album picker without `album`, no records line without `records`.

Where the import goes is one pop-up button, `to`: This Mac or Cloud. It shows when the library has cloud storage (`cloud`), and it is one stored choice: the bulk pane and Settings → Import show the same one. `retry` shows Retry import in Import's place, and `blank` shows neither while the folder has nothing to import.

A library release's editor draws its card with `MacReleaseEditorCard`, and a skipped folder's pane with `MacInvalidCandidateCard`.

**In the code:** macOS: `MacImportDraft` (`Views/Import/Mapping/MacImportDraft.swift`), with `ReleaseMetadataHeader` (`Views/Components/MetadataForm/ReleaseMetadataHeader.swift`).
