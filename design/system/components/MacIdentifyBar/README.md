# MacIdentifyBar

The draft card's top row.

- Identify, then Automatic, Search and the ⋯ button that opens the Metadata menu.
- At the right, where the import goes (`to`: This Mac or Cloud) when the library has cloud storage (`cloud`), then Import, or Retry import after an import failed (`retry`). A folder with nothing to import (`blank`) shows no Import.
- In a library release's editor (`editor`): no Metadata menu, and Save, with Cancel once something changed (`changed`). While a save reads a file on this device before it writes, such as the audio file a CUE sheet is newly bound to, a spinner and `reading` (Reading Northbound (2019 rip).flac…) sit before Cancel and Save, which are off until the read ends.

**In the code:** macOS: `ImportCommitControls` and `ImportReleaseSourceActions` (`bae-macos/bae/bae/Views/Import/Mapping/MacImportDraft.swift`)
