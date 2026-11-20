# AndroidDisclosure

Material's disclosure row: its own card, the label (and an optional `summary` under it), `expand_more` at the end when closed and `expand_less` when open. The rows it shows join the card on the same text edge, tiled like any card's rows. Children are `AndroidRow`s.

On macOS the same thing is `MacDisclosure`.

**In the code:** Android: `AndroidDisclosure` (`ui/components/AndroidDisclosure.kt`), a row with Material's expand more and expand less glyphs that opens its rows in the same card; `FormerDevices` (`ui/settings/FormerDevices.kt`) holds whether it is open.
