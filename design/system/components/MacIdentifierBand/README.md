# MacIdentifierBand

The identifier chips: `chips` as "Label|value|filled|outlined|struck|dash|Source=count+Source=count", comma-separated. A chip holding several values (the folder's ISRCs, one per track; catalog numbers no release confirms) lists every one, separated by " · " and wrapping inside the chip: never the first alone, never "+n" or a count behind a disclosure. A chip is filled for a value the run asks about, outlined for a Disc ID or barcode the run leaves out, and struck through for a catalog number the person struck out.

Each chip is a `MacIdentifierChip`.

**In the code:** macOS: `MacIdentifierBand` and `MacIdentifierChip` (`Views/Import/Search/`).
