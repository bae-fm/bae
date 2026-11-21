# MacIdentifyBar

The release editor's top row, in both of its action rows (`actions`).

- Identify, then Automatic, Search and the ⋯ button that opens the Metadata menu, in both rows.
- When the pane is too narrow for the whole row, the right-hand group (the destination and Import or Save) wraps onto a second line, at the right.
- `actions` `import`: at the right, where the import goes (`to`: This Mac or Cloud) when the library has cloud storage (`cloud`), then Import, or Retry import after an import failed (`retry`). A folder with nothing to import (`blank`) shows no Import.
- `actions` `save`: Save, with Cancel once something changed (`changed`). `reading` (Reading Northbound (2019 rip).flac…) shows a spinner and the file a save or an import reads before it writes, before the buttons, which are off until the read ends, as the Metadata menu is.

**In the code:** macOS: `MacIdentifyBar` (`bae-macos/bae/bae/Views/Import/Mapping/MacIdentifyBar.swift`), in `MacReleaseEditor`.
