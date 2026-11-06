# MacDetailRow

A row of a composer's page or a work's: 44pt cover, title, a second line that holds its height when empty. Works and releases open on click (hover fill bleeds 8pt past the text); `credit` is the text-only row for a credit or a recording, which opens nothing. `part` indents a work 16pt under its group's heading work. `top` is the gap above the row: the composer page sets 1 or 16 for a work, 24 for a credit not under a heading.

**In the code:** macOS: `DetailMediaRow` in `DetailRowButtonStyle`, and `CreditRow` (`Views/Library/Browse/`).
