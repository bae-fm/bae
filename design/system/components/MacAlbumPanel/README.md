# MacAlbumPanel

The album opened in the grid, in a panel under its row: the 340pt cover (opens the gallery), title, artists · year, the release picker when there are several, the pressing and labels, why this Mac can't play it when it can't, Play and the album's menu, the rating row, then the tracks. Close in the corner.

The panel draws its head with `MacAlbumHeader`, which takes the same props: `releases` and `release` for `MacReleasePicker`, `options`, `optionsOpen` and `highlighted` for the album's ⋯ (`MacMoreMenu`), and `rating` (0 while no one has rated it) and `ratingWho` for `MacAlbumRating`, under Play. A board gives the album's tracks, credits and works as children.

**In the code:** macOS: `AlbumExpansionContent` (`Views/Library/AlbumDetail/AlbumExpansionContent.swift`) inside `AlbumDetailView`.
