# MacLibraryRow

One row of a library list on the Mac: Search's results, the artist and composer lists, a composer's page and a work's. A 44pt cover 12pt from the text; the title over a second line 2pt below, which holds its height when empty; `trailing` for a track's duration. The row is padded 8pt; the list it sits in lines its headings up with that. `hover` fills it; `selected` tints it while its page is open beside the list; `loading` is the row before it has arrived.

On a page rather than in a list, `bleed` lets the hover fill reach 8pt past the text on each side, so the text lines up with the page's headings. `credit` is the text-only row of a credit or a recording, 4pt above and below, which opens nothing. `part` indents a work 16pt under its group's heading work; `top` is the gap above the row, which the page sets per row (1 or 16 before a work, 24 before a credit not under a heading).

**In the code:** macOS: `MacLibraryRow` (`Views/Components/MacLibraryRow.swift`); `MacLibraryRowButton` is the row that opens something, owning its hover. Its shots are `mac.component.library-row.<state>` (`baeShots/ShotScene+Components.swift`).
