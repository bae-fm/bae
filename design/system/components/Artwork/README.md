# Artwork

A cover, an artist's picture or a composer's. Boards give each title its own stand-in colors, so a page reads like a library. `missing` is a release or person with no image: a plain tile with no glyph, as both apps draw it. A cover still loading looks the same. `ring` draws the open album tile's accent ring around the cover.

**In the code:** macOS: `ImageView` (`Views/Components/ImageView.swift`), clipped by the caller. Android: `CoverImage` (`ui/components/CoverImage.kt`). A cover that fails to load shows a retry glyph in both; not drawn here.
