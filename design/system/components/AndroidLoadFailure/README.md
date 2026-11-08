# AndroidLoadFailure

A view that failed whole, drawn quietly with no box and no glyph, centred in what is left of the view: the message in secondary text (16px), then one row under it with “Show details” in accent (14px) and Retry, an outlined `AndroidButton`, last. Open, the link reads “Hide details” and the causes sit under the row in the same inset panel as macOS (about 340 wide, left-aligned), with Copy in its corner. The macOS twin is `MacLoadFailure`.

| Prop | |
| --- | --- |
| `message` | What failed to load. |
| `chain` | The causes, one a line, ending in the fault; with it there is a “Show details”. |
| `open` | The causes shown. |

**In the code:** Android has no shared view for this yet; the library screen draws its own failure.
