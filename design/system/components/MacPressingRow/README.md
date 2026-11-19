# MacPressingRow

A pressing: year, labels, catalog number, the agreement badges, the summary; In library (dimmed) only when the library holds it, the catalogs that have answered for it (`sources`, none until one has), a chevron. Where the pressing's records state a fact differently (year, catalog number, barcode, country, format), the row shows every record's value, each followed by its catalog's name ("1992 MusicBrainz / 1993 Discogs"): written `1992:MusicBrainz|1993:Discogs`, in `summary` as one of its parts. A fact one record states alone is shown as stated, and the labels list every record's labels. `note` is the records' notes under the summary, every record's that states one, never the first alone. When the folder's own records carry notes too, a Notes badge follows the other badges, and pointing at it shows those notes (`notes`).

Each fact is a `MacSourcedValue`.

**In the code:** macOS: `ImportSearchResultRow` (`Views/Import/Search/`).
