# MacIdentifierBand

The identifier chips: `chips` as "Label|value|filled|outlined|struck|dash|Source=count+Source=count", comma-separated. A chip holding several values (the folder's ISRCs, one per track; catalog numbers no release confirms) lists every one, separated by " · " and wrapping inside the chip: never the first alone, never "+n" or a count behind a disclosure. A chip is filled for a value the run asks about, outlined for a Disc ID or barcode the run leaves out, and struck through for a catalog number the person struck out.

Each chip is a `MacIdentifierChip`. Each Disc ID, Barcode and Catalog # chip that holds a value toggles: pressing it leaves that identifier out of the run, or puts it back. The Title search chip's words are a button that opens its artist and title fields. The ISRC and dash chips are not buttons.

**In the code:** macOS: `MacIdentifierBand` and `MacIdentifierChip` (`Views/Import/Search/`).
