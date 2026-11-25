# MacIdentifierChip

One identifier the folder's files state, in the identifier band over Find online's results: its label in small tertiary type, then its value in mono (`label`, `value`).

A value longer than 92pt ends in an ellipsis. Of several values, separated by " · ", as the ISRC chip holds, the chip shows the first, then a +N button that opens a popover listing every one, a line each (decision #2264). `moreShown` draws that popover open, after the chip. The band sets `moreOpen` instead: the chip draws its button open and the band draws the popover.

- `state`: `filled` (the default), on the hover fill; `outlined`, a hairline ring; `struck`, its value struck through; `dash`, a dash for an identifier the files leave out.
- `sources`: the catalogs searched for the identifier, each a capsule with how many releases it found ("MusicBrainz=2,Discogs=0", comma-separated). A count of … shows Searching… while the search runs.
- `toggles`: the chip is a button. Pressing it leaves its identifier out of the run, or puts back one left out or struck out. The button reports no pressed state: the fill, the ring and the strike show it.
- `editable`: the value is a button that opens fields to change it, as the title search's words open its artist and title fields.

**In the code:** macOS: `MacIdentifierChip` and `IdentifierDash` (`bae-macos/bae/bae/Views/Import/Search/MacIdentifierChip.swift`).
