# PlaylistPicture

A playlist's picture: the covers of its first four different releases in a 2×2 square, as playlists show on both platforms. With fewer than four, its first entry's cover; with none, the missing tile, as `Artwork` draws it. One cover alone would make a playlist look like that album, and two playlists that start on one album alike. `covers` lists the releases' titles in the playlist's order, each once; `size` and `radius` as `Artwork`'s. Every playlist row (`MacLibraryRow`, `AndroidLibraryRow` with `covers`) and a playlist's header draw it. It is decorative, as a cover beside its title is.

**In the code:** not built yet: the playlist rows and header draw one cover.
