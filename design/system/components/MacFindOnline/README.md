# MacFindOnline

Identify: the Back button and Identify, the Discogs bar, a `MacNotice` with Open settings and the × that dismisses it, then Automatic and Search, each heading a button that opens its section, one open at a time. The open section's button reads selected. The children fill Search when it is open.

Automatic draws itself from its run's state (`automatic`), as the app's MacFindOnline draws from the run:

| `automatic` | Header | Section |
| --- | --- | --- |
| `not-started` | nothing | Search manually alone |
| `working` | the spinner | the band, then the children |
| `failed` | the warning glyph | the band, then the children |
| `nothing-found` | 0 | the band, then No results and Search manually |
| `found` (the default) | nothing | the band, then the children: what the run found |

`chips` is the run's identifier band, a `MacIdentifierBand`, with a rule under it.

**In the code:** macOS: `MacFindOnline` (`Views/Import/Search/MacFindOnline.swift`), drawn from values, with `MacFindOnlineContainer` reading the Discogs bar's state for it, and `FindOnlineHeader`, `FindOnlineDiscogsBar`, `FindOnlineSectionHeader`, `FindOnlineAutomaticSection` and `FindOnlineSearchResults` beside it.
