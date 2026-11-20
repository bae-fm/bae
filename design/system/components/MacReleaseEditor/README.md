# MacReleaseEditor

The one editor of a release, in the import pane (a found folder's draft or an imported folder) and in the album's Edit release sheet. The two differ only in the action row, `actions`.

- **`import`:** where the import goes (`to`: This Mac or Cloud, shown when the library has cloud storage, `cloud`) and Import, or Retry import (`retry`). A folder with nothing to import (`blank`) shows neither. The folder's `check` sits in a warning `MacNotice` under that row.
- **`save`:** Cancel once something changed (`changed`), and Save.

`reading` shows a spinner and the file a save or an import reads before it writes, before the buttons, which are off until the read ends.

Everything else is the same in both:

- **Identify:** Automatic, Search and the ⋯ button that opens the Metadata menu (apply the files' tags, clear the metadata), in a `MacIdentifyBar`.
- **Card:** the cover (`cover`, or a plain tile) and a `MacReleaseIdentity`: the album title (`title`), the album artist (`artist`, with `artistChip`), the album's year (`year`), the audio line (`audio`: `FLAC · 44.1 kHz · 16-bit · Stereo`) and a `MacAlbumPicker` (`album`, with `albumInLibrary`, `albumPicked` and `albumOpen`). Then a `MacReleaseFields`: Year (`pressingYear`), Media (`media`), Label (`labels`), Country (`country`, with `countryOpen`), Barcode (`barcodes`), Status (`status`), Packaging (`packaging`) and Details (`details`). The card ends with a `MacRecordLinks` (`records`) and Unlink, which drops the link and its records.
- **Gallery:** `images`, a `MacImportImages`, above the tracks.
- **Track table:** the children, in a `MacImportTable`: side headings, `MacImportSheetCaption` rows for CUE sheets and `MacImportTrackRow` rows.
- **Files:** `files`, a `MacReleaseFiles`, under the tracks.

A part with no value draws nothing: no audio line without `audio`, no album picker without `album`, no records line without `records`, no gallery without `images`, no table without children, no Files list without `files`.

A skipped folder's pane is read-only and uses `MacInvalidCandidateCard`.

**In the code:** macOS: `MacReleaseEditor` (`Views/Components/MetadataForm/MacReleaseEditor.swift`), whose `actionRow` is `importing` in the import pane (`ImportMappingPane`) and `saving` in the Edit release sheet and an imported folder's pane (`ReleaseEditorView`).
