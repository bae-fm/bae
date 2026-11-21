# MacPressingRow

A pressing: its edition line (`edition`: the year, each label and its catalog number, then the country in its short form, joined with ` · `), then its copy line (`copy`: the media and details) with the barcode and the agreement badges after it; In library (dimmed) only when the library holds it, the catalogs that have answered for it (`sources`, none until one has), a chevron. Where the pressing's records state a fact differently (year, catalog number, barcode, country, format), the row shows every record's value, each followed by its catalog's name ("1992 MusicBrainz / 1993 Discogs"): written `1992:MusicBrainz|1993:Discogs`, in `edition` or `copy` as one of its parts. A fact one record states alone is shown as stated, and the labels list every record's labels. `notes` is the records' notes the folder's text names the pressing by, every record's that states one, never the first alone: they show under the copy line, and a Notes badge follows the other badges, with the same notes shown when the pointer rests on it. The badge and the line always show together.

Each fact is a `MacSourcedValue`.

**In the code:** macOS: `MacPressingRow` (`Views/Import/Search/MacPressingRow.swift`), under each `MacReleaseCard` in `MacReleaseGroup`.
