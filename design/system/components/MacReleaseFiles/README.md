# MacReleaseFiles

The files a release carries that are no track, such as a rip log or a cover image: a ruled Files heading, then one line per file, its name in mono and its size at the right, a hairline between lines. A document opens when clicked. The same part wherever a release's files show: a Found folder's pane, an imported release's pane, and the album's Edit Release sheet, each under the tracks. The apps draw all three with one view: never a second way to list a release's files.

**In the code:** macOS: `ImportedReleasePane.filesSection` (`Views/Import/Mapping/ImportedReleasePane.swift`); the Found pane's table draws its own Files rows today (`ImportMappingTable`), and the Edit Release sheet lists no files yet.
