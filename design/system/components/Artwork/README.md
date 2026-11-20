# Artwork

A cover, an artist's picture or a composer's. Boards give each title its own stand-in colors, so a page reads like a library. `missing` is a release or person with no image: a plain tile with no glyph, as both apps draw it. A cover still loading looks the same. `ring` draws the open album tile's accent ring around the cover.

Beside its title, as in a row, a tile, a heading or the player, the picture is decorative and screen readers skip it, as both apps mark it. `label` names one that is itself what the screen shows, such as a gallery's image.

**In the code:** macOS: `Artwork` (`Views/Components/Artwork.swift`). Android: `Artwork` (`ui/components/Artwork.kt`). A cover that fails to load shows a tile to tap to retry on both; the preview does not draw that state.
