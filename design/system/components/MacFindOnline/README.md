# MacFindOnline

Identify: the Back button and Identify, the Discogs bar, a `MacNotice` with Open settings and the × that dismisses it, then Automatic and Search, each heading a button that opens its section, one open at a time; children fill the open one. Automatic's header ends in what its run has to say (`autoStatus`): the spinner while it works (`working`), the warning glyph when it failed (`failed`), or 0 when nothing matched (`empty`).

**In the code:** macOS: `MacFindOnline` (`Views/Import/Search/MacFindOnline.swift`), drawn from values, with `MacFindOnlineContainer` reading the Discogs bar's state for it, and `FindOnlineHeader`, `FindOnlineDiscogsBar`, `FindOnlineSectionHeader`, `FindOnlineAutomaticSection` and `FindOnlineSearchResults` beside it.
