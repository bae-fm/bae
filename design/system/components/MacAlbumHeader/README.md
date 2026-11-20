# MacAlbumHeader

The open album's head, beside its cover, from the top:

- the title, then the artist and the year;
- the release picker (`MacReleasePicker`), when the album has more than one release (`releases`, with `release` the one showing);
- the facts: the pressing line (`pressing`) and the labels (`labels`);
- the line that says where the album is, when this Mac can't play it (`unplayable`);
- Play and the album's ⋯ (`MacMoreMenu`, boxed), whose menu holds `options`; `optionsOpen` draws it open, `highlighted` the item under the pointer;
- the rating row (`MacAlbumRating`), always: the stars (`rating`, empty while no one has rated it) and `ratingWho`, the line of who else rated it.

**In the code:** macOS: `MacAlbumHeader` (`Views/Library/AlbumDetail/MacAlbumHeader.swift`), with `ReleaseFactsLine` for the release's facts, inside `MacAlbumPanel`.
