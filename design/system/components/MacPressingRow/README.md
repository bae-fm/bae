# MacPressingRow

A found pressing, a row of a `MacPressingTable`: the year in strong text, the label in secondary text, then the catalog number, the country in its short form, the format and the details in tertiary text, each in its own column (`year`, `label`, `catalog`, `country`, `format`, `details`). A value longer than its column ends in an ellipsis. Then a dot in each identifier column whose kind names the pressing (`matches`, comma-separated), an empty cell in the others, and the chevron.

Where the pressing's records state a fact differently, the cell shows every record's value, each followed by its catalog's name ("1992 MusicBrainz / 1993 Discogs"), written `1992:MusicBrainz|1993:Discogs`. A fact one record states alone is shown as stated. Each fact is a `MacSourcedValue`.

`notes` is the records' notes the folder's text names the pressing by, every record's that states one, in italics after the details. `inLibrary` dims a pressing the library holds and adds In library with its check after the details. `selected` marks the row picked.

**In the code:** macOS: `MacPressingRow` (`Views/Import/Search/MacPressingRow.swift`), under each `MacReleaseCard` in `MacReleaseGroup`.
