# MacAlbumHeader

The open album's head, beside its cover, from the top:

- the title, the artist and the year, each on its own line;
- the release picker (`MacReleasePicker`), when the album has more than one release (`releases`, with `release` the one showing);
- the release lines, two lines. The edition line (`edition`): the release's own year, even when it matches the album's, its country in the short form the person's language commonly uses, then each label with its catalog numbers, as in `2001 · US · Harbor Records · HR-1042`;
- The copy line (`copy`): the media and details, the audio and the length, as in `2×LP · Remastered · FLAC · 52 min`. Core words both lines, and Android shows the same words. A line with nothing in it is left out;
- the line that says where the album is, when this Mac can't play it (`unplayable`);
- Play and the album's ⋯ (`MacMoreMenu`, boxed), whose menu holds `options`; `optionsOpen` draws it open, `highlighted` the item under the pointer;
- the rating row (`MacAlbumRating`), always: the stars (`rating`, empty while no one has rated it) and `ratingWho`, the line of who else rated it.

**In the code:** macOS: `MacAlbumHeader` (`Views/Library/AlbumDetail/MacAlbumHeader.swift`), with `ReleaseFactsLine` for the release's facts, inside `MacAlbumPanel`.
