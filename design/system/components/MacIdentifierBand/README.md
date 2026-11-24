# MacIdentifierBand

The identifier chips: `chips` as "Label|value|filled|outlined|struck|dash|Source=count+Source=count", comma-separated. A chip holding several values (the folder's ISRCs, one per track) lists every one, separated by " · " and wrapping inside the chip. A chip is filled for a value the run asks about, outlined for a Disc ID or barcode the run leaves out, and struck through for a catalog number the person struck out. An outlined Catalog # chip is a catalog number the folder offers that no release confirms.

Each chip is a `MacIdentifierChip`. Each Disc ID, Barcode and Catalog # chip that holds a value toggles: pressing it leaves that identifier out of the run, or puts it back. Pressing an outlined Catalog # chip puts its number into the run.

The band folds the outlined Catalog # chips after the other chips, behind a "+N more" button with their count (decision #2237). `candidates="shown"` draws every one, then the button, which then reads Fewer and folds them again. The Title search chip's words are a button that opens its artist and title fields. The ISRC and dash chips are not buttons.

**In the code:** macOS: `MacIdentifierBand` and `MacIdentifierChip` (`Views/Import/Search/`).
