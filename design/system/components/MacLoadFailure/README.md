# MacLoadFailure

A view that failed whole (the album grid, the import list, an image in the lightbox), drawn quietly with no box and no glyph, centred in what is left of the view. The message in secondary text, 15px, no period; under it one row: “Show details” in accent, then Retry as a see-through button, last. Open, the link reads “Hide details” and the causes sit under the row in the inset panel, about 340 wide, left-aligned, with Copy in its corner.

| Prop | |
| --- | --- |
| `message` | What failed to load. |
| `chain` | The causes, one a line, ending in the fault; with it there is a “Show details”. |
| `open` | The causes shown. |

In the lightbox the same drawing sits on the dark stage, in the stage's text colors.

**In the code:** macOS: BaeKit's `LoadFailureView`.
