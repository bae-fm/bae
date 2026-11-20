# MacSearchSourceLine

One catalog that a typed search in Identify still waits on, under the results, one line a catalog, set in from the results' edge. While the catalog searches, the line is a spinner and "Searching {catalog}…". When the catalog fails, the line is a warning mark, the catalog's name with why it failed on hover, and Retry. A catalog that answered, or that the search never asked, has no line.

| Prop | What it does |
|---|---|
| `catalog` | The catalog's name: MusicBrainz or Discogs. |
| `failed` | The catalog did not answer: the warning mark, its name and Retry. |
| `reason` | Why it failed, as the name's hover text. |

**In the code:** macOS: `MacSearchSourceLine` (`Views/Import/Search/MacSearchSourceLine.swift`), the `sourceLines` of `FindOnlineSearchResults` (`Views/Import/Search/FindOnlineSearchResults.swift`).
