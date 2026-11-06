# AndroidSearchRow

One row of a list: picture (none for a track or a credit), title, second line (left out when empty, so the row is shorter), a track's duration. Search's results, and the composer and work pages' rows. `part` indents a work 16dp under its group's heading work; `inset` is those pages' rows today, whose text sits 16dp further in, in a taller row.

**In the code:** Android: the result rows in `SearchResultsScreen` (`ui/library/SearchResultsScreen.kt`); artists and composers use `ArtistSummaryRow` and `ComposerSummaryRow`. On the composer and work pages, `WorkSummaryRow`, the release rows and `TwoLineText` (`ui/library/ComposerDetailScreens.kt`): `inset` draws their text's second padding.
