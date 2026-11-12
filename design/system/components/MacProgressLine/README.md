# MacProgressLine

A line of progress, in the surrounding font: the phase, a bar filling the rest of the line, and an optional count ("112 / 130"). The phase and the count are secondary text; the bar is 4pt high and never narrower than 48pt. A phase too long for the line ends in an ellipsis before the bar gives way, and a line too narrow for all three drops the count. Leave out `value` when the length isn't known: the bar marches. `activity` fills the bar in the activity colour, for files moving to or from storage.

A running import's step in its row, a row's upload, and the identify popover's "Identifying [bar] 112 / 130". For a step's name over a wider bar, on its own in a pane or sheet, use `MacProgress`.

**In the code:** macOS: `ProgressLine` (`Views/Components/ProgressLine.swift`), in `ImportProgressLine` (`TriageRowView`, `ImportedReleasePane`), `TriageRowView`'s upload line and `IdentificationProgressView`.
