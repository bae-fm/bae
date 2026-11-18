# MacImportDraft

The draft card: Identify with Automatic, Search and the ⋯ button that opens the Metadata menu; where the import goes and Import (or Retry import; `editor` shows Cancel and Save as the library's release editor does); the folder's `check` in a warning `MacNotice` under that row; the cover and the identity; the Release grid; the linked records with Unlink.

- **Cover:** `cover`, or a plain tile when the draft has none. The tile says nothing and invites nothing.
- **Identity:** the album title (`title`) and the album's year (`year`) are fields, drawn as their values until the pointer is over one. The album artist (`artist`) is a button, with its `artistChip` (Library or New) inside it. Under them the audio line (`audio`, as the scan read the folder: `FLAC · 44.1 kHz · 16-bit · Stereo`), then the album pop-up, named Album, showing the album the release files under (`album`). `albumOpen` with `albumItems` draws the album picker open: New album, the library album the records name, and Choose another album…, which searches the library's albums.
- **Release grid:** fields for Year (`pressingYear`), Label (`labelName` and `catalog`, a field each), Barcode (`barcode`) and Details (`details`); pop-ups, each named for its row, for Media (`media`), Country (`country`), Status (`status`) and Packaging (`packaging`). An empty field or pop-up shows a dash.
- **Records:** the record links, then Unlink, a button.

A part with no value draws nothing: no audio line without `audio`, no album pop-up without `album`.

Where the import goes is one pop-up button, `to`: This Mac or Cloud. It shows when the library has cloud storage (`cloud`), and it is one stored choice: the bulk pane and Settings → Import show the same one.

**In the code:** macOS: `MacImportDraft` and `ReleaseMetadataHeader` (`Views/Import/Mapping/`, `Views/Components/MetadataForm/`); `ReleaseEditorView` for `editor`.
