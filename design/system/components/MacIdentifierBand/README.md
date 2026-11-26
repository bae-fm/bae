# MacIdentifierBand

The identifier chips: `chips` as "Label|value|state[|trailing]", comma-separated, each state `off`, `on`, `found` or `dash` (`MacIdentifierChip`). The trailing is `searching` while a lookup of the value is out, or `failed` once one failed, written `failed:reason` to give the warning's hover its words. The ISRC chip holds the folder's ISRCs, one per track, separated by " · ": it shows the first, then a +N button that opens a popover listing every one, a line each (decision #2264). `open` names the chip whose popover is drawn open, ISRC. The popover is a window of its own, so it comes after the whole band in the reading order.

Each Disc ID, Barcode and Catalog # chip that holds a value toggles: pressing it leaves that identifier out of the run, or puts it back. The ISRC and dash chips are not buttons.

`titleSearch` is the words the run searches by, "artist|title|state": one chip after the others that reads Artist, the artist's field, Title, the title's field, `found` unless given (decision #2561).

`offered` is the catalog numbers the folder offers that no release confirms, comma-separated. Each is an off Catalog # chip, and pressing it puts its number into the run. The band folds them after the other chips, behind a "+N more" button with their count (decision #2237). `candidates="shown"` draws every one, then the button, which then reads Show fewer and folds them again.

**In the code:** macOS: `MacIdentifierBand` and `MacIdentifierChip` (`Views/Import/Search/`).
