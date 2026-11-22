# MacIdentifyBar

The release editor's top row, in both of its action rows (`actions`).

- Identify, then Automatic, Search and the ⋯ button that opens the Metadata menu, in both rows.
- When the pane is too narrow for the whole row, the right-hand group (the destination and Import or Save) wraps onto a second line, at the right.
- `actions` `import`: at the right, where the import goes (`to`: This Mac or Cloud) when the library has cloud storage (`cloud`), then Import, or Retry import after an import failed (`retry`). A folder with nothing to import (`blank`) shows no Import.
- `actions` `save`: Save, with Cancel once something changed (`changed`). When a save reads an audio file, `reading` shows a spinner and the filename before the buttons. The editor disables the buttons and the Metadata menu until the read ends.

**In the code:** macOS: `MacIdentifyBar` (`bae-macos/bae/bae/Views/Import/Mapping/MacIdentifyBar.swift`), in `MacReleaseEditor`.
