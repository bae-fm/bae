# MacReleaseFiles

The files a release carries that are no track, such as a rip log or a cover image: a ruled Files heading, then one line per file, its name in mono and its size at the right, a hairline between lines. A document opens when clicked. The same part wherever a release's files show: a Found folder's pane, an imported release's pane, and the album's Edit Release sheet, each under the tracks. The apps draw all three with one view: never a second way to list a release's files.

**In the code:** macOS: `ReleaseFilesView` (`Views/Import/Mapping/ReleaseFilesView.swift`), drawn under the Found pane's table (`ImportMappingTable`) and under the release editor's tracks (`ReleaseMetadataEditorContent`), which an imported release's pane and the Edit Release sheet both show.
