# MacReleaseFields

The release's fields under a Release heading, in a grid of labels and values, as the Mac editor draws them. Each list adds an entry with the same control, + and its words in the accent, and removes one with ⊗.

- **Year** (`year`): a `MacInlineField`.
- **Media** (`media`, "2×CD,1×DVD"; a carrier alone is one of it): on one line, each carrier's name, ×count, a `MacStepper` and ⊗, then Add, a menu of the carriers not listed yet.
- **Label** (`labels`, "Name|Catalog", comma-separated): one row per label, its name and its catalog number (in mono) each a `MacInlineField` as wide as its value, then ⊗; Add label on its own line under the last row, or beside the name while there is none.
- **Country** (`country`): the country's name as plain text, which opens the menu of countries. `countryOpen` (the countries it lists) draws that menu open under it, the dash for none first and the scroll arrow at its foot.
- **Barcode** (`barcodes`, comma-separated): one row per barcode, a mono field, then ⊗; Add barcode under the last row.
- **Status** (`status`) and **Packaging** (`packaging`): each its value as plain text, which opens its menu.
- **Details** (`details`, comma-separated, such as Remastered, Limited Edition): what Discogs says about the pressing that no other field holds, each with ⊗, then a menu that adds another: named Details and showing a dash while there are none, named Add detail and showing + Add after.

An empty value shows the form's dash.

**In the code:** macOS: `MacReleaseFields` (`Views/Components/MetadataForm/MacReleaseFields.swift`), with `MediaCountsEditor` (`Views/Components/MetadataForm/MediaCountsEditor.swift`) and `DiscogsDetailsEditor` (`Views/Components/MetadataForm/DiscogsDetailsEditor.swift`).
