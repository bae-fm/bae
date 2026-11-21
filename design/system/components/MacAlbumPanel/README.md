# MacAlbumPanel

The album opened in the grid, in a panel under its row: the 340pt cover (opens the gallery), title, artist and year each on its own line, the release picker when there are several, the edition line and the copy line, why this Mac can't play it when it can't, Play and the album's menu, the rating row, then the tracks, and last the audio line (`audio`, decision #1232). Close in the corner.

The panel draws its head with `MacAlbumHeader`, which takes the same props: `releases` and `release` for `MacReleasePicker`, `options`, `optionsOpen` and `highlighted` for the album's ⋯ (`MacMoreMenu`), and `rating` (0 while no one has rated it) and `ratingWho` for `MacAlbumRating`, under Play. A board gives the album's tracks, credits and works as children; the audio line follows them.

**In the code:** macOS: `MacAlbumPanel` (`Views/Library/AlbumDetail/MacAlbumPanel.swift`) inside `AlbumDetailView`.
