# MacIdentifierChip

One identifier the folder's files state, in the identifier band over Find online's results: its name in the chip type, then its value on a dark inset of the chip's fill, the background at 70%, in the detail type (`label`, `value`). The chip names no catalog and shows no count (decision #2561).

Of several values, separated by " · ", as the ISRC chip holds, the chip shows the first, then a +N button in the name's colour that opens a popover listing every one, a line each, in the value's type (decision #2264). `moreShown` draws that popover open, after the chip. The band sets `moreOpen` instead: the chip draws its button open and the band draws the popover.

- `state`: the chip's look (decision #2561).
  - `off`: an outline, with the name and value in tertiary text, for an identifier the run leaves out. A catalog number the person left out of the run is an off chip, and so is a catalog number the folder offers that no release confirms.
  - `on` (the default): the hover fill, with the name in tertiary text and the value halfway between primary and secondary text, for an identifier the run asks about.
  - `found`: the accent at 26%, with the name in primary text at 75% and the value in primary text, for an identifier that found releases.
  - `dash`: a dash after the name for an identifier the files leave out, with no fill and no ring.
- `toggles`: the chip is a button. Pressing it leaves its identifier out of the run, or puts back one left out. The button reports no pressed state: the look shows it.
- `fields`: the title search's words, `Artist=…,Title=…`. The chip reads Artist, the artist's field, Title, the title's field, in place of a name and value, and each field draws as a value does.

**In the code:** macOS: `MacIdentifierChip` and `IdentifierDash` (`bae-macos/bae/bae/Views/Import/Search/MacIdentifierChip.swift`).
