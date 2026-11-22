# MacAlbumHeader

The open album's head, beside its cover, from the top:

- the title, the artist and the year, each on its own line;
- the release picker (`MacReleasePicker`), when the album has more than one release (`releases`, with `release` the one showing);
- the release lines, two lines. The edition line (`edition`): the release's own year, left out when it matches the album's year above it, each label with its catalog numbers, then its country in the short form the person's language commonly uses, as in `2001 · Harbor Records · HR-1042 · US`;
- The copy line (`copy`): the media and details, then the length, as in `2×LP · Remastered · 52 min`. The audio line ends the album page instead (`MacAlbumPanel`'s `audio`). Core words the lines, and Android shows the same words. A line with nothing in it is left out;
- when only another device holds the files (`unplayable`, the device), the words Not on this device in place of Play, whose `MacHelpTag` names the device (`unplayableHover` draws it showing);
- Play and the album's ⋯ (`MacMoreMenu`, boxed), whose menu holds `options`; `optionsOpen` draws it open, `highlighted` the item under the pointer;
- after the ⋯, the person's own rating when they have rated the album: the number and one star in the accent, read-only (`rating`, "4 ★", decisions #1219 and #1233). `ratingSystem` is the library's system for albums: `heart` draws one filled heart there at three stars or more and nothing below, and `none` draws nothing.

Rating, and every other action beyond Play, is an item of the ⋯ menu: Rate opens a submenu (decision #1217).

**In the code:** macOS: `MacAlbumHeader` (`Views/Library/AlbumDetail/MacAlbumHeader.swift`), with `ReleaseFactsLine` for the release's facts, inside `MacAlbumPanel`.
