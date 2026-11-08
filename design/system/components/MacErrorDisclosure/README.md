# MacErrorDisclosure

BaeKit's error disclosure: the line, then Details, which opens on an inset panel of the causes, one a line and ending in the fault, with Copy details in its corner. It is drawn in a `Notice` of its tone, the notice's glyph leading, so an open Details stays inside the box. On the tint the line is primary text and the greys are primary text made see-through. Details starts in line with the message; its chevron hangs in the margin to the left, so the words share one edge.

| Prop | |
| --- | --- |
| `tone` | `danger` (the default), `warning`, or `neutral` for a reason, which has no glyph. |
| `fault` | The deepest cause; with it there is a Details. |
| `open`, `chain` | Details open; `chain` is the causes, one a line, ending in the fault (without it, the fault alone). |
| `action` | The label of a button (Retry) drawn in the box, top-right, level with the message; an open Details panel runs the box's full width under it. |
| `bare` | No notice: for a disclosure inside a notice that already holds it, under a title (the launch problem, the welcome screen's failures). |

**In the code:** macOS: BaeKit's `ErrorDetailDisclosure` (`BaeKit/Sources/BaeKit/Views/ErrorDetailDisclosure.swift`). The app draws it bare in most places and inside `.notice(_:)` in some; the canvas draws the notice everywhere.
