# MacReleaseFields

The release's fields under a Release heading, in a grid of labels and values: Year (`year`), Media (`media`), Label with its catalog number (`labelName`, `catalog`), Country (`country`), Barcode (`barcode`), Status (`status`), Packaging (`packaging`) and Details (`details`). The typed values are `MacInlineField`s; Media, Country, Status and Packaging are pop-ups, each named for its row. An empty field or pop-up shows a dash.

Details holds what Discogs says about the pressing that no other field holds (`details`: comma-separated, such as Remastered, Limited Edition): each detail with a button that removes it, then a menu that adds another, showing a dash while there are none and + Add after.

**In the code:** macOS: `ReleasePressingFieldsGrid` (`Views/Components/MetadataForm/ReleaseMetadataHeader.swift`), with `DiscogsDetailsEditor` (`Views/Components/MetadataForm/PressingFactPickers.swift`) for Details.
