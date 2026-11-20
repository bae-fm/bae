# MacReleaseFiles

The files a release carries that are no track, such as a rip log or a track sheet that carves no tracks. No line is an image bae can show (a JPEG, PNG, GIF or WebP): the release's images above the tracks show those. An image in another format, such as a BMP, is a plain `other` line. A ruled Files heading, then one line per file, its name in mono and its size at the right, a hairline between lines. Each line is a button named by its file: a document or a track sheet opens in the viewer. A third part says what else a file is (`files`: "name|size|kind"):

- `other`: a file that opens nothing, a plain line.
- `audio`: a track on a skipped folder's pane, which lists every file but the images bae can show; its Play button sits before the size.
- `damaged:why`: a damaged file, marked with a warning glyph and why, opening nothing. The same part wherever a release's files show: a Found folder's pane, an imported release's pane, and the album's Edit Release sheet, each under the tracks. The apps draw all three with one view: never a second way to list a release's files.

The heading is a `MacFormHeading`, and each line a `MacReleaseFileRow`.

**In the code:** macOS: `ReleaseFilesView` (`Views/Import/Mapping/ReleaseFilesView.swift`), drawn under the Found pane's table (`ImportMappingTable`) and under the release editor's tracks (`ReleaseMetadataEditorContent`), which an imported release's pane and the Edit Release sheet both show.
