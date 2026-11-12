# AndroidLibraryRow

One row of a library list: picture, title, second line (left out when empty, so the row is shorter), and a track's duration at the end. Search's results, the artist and composer lists, and the composer and work pages' rows all draw it. `noArt` for a track or a credit, which have no picture; `missing` for an artist or composer with no picture, a plain tile. `part` indents a work 16dp under its group's heading work; `inset` is the composer and work pages' rows, whose text sits 16dp further in, in a taller row.

**In the code:** Android: `ArtistSummaryRow` and `ComposerSummaryRow` (`ui/library/ArtistBrowser.kt`, `ComposerBrowser.kt`), identical; the result rows in `SearchResultsScreen` (`ui/library/SearchResultsScreen.kt`); `WorkSummaryRow`, the release rows and `TwoLineText` on the composer and work pages (`ui/library/ComposerDetailScreens.kt`).
