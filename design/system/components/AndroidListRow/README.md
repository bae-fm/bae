# AndroidListRow

Exploration: a history or playlist row on Android.

`menu` ends the row in ⋮, the row's actions; a text value labels the button (More unless given). `dragging` is the row under the owner's finger while they drag a playlist's track to a new place: raised over the rows on the elevated colour, with a shadow and rounded corners, as the app's `PlaylistEntry` draws a lifted row.

**In the code:** Android: `AndroidListRow` (`ui/components/AndroidListRow.kt`), in a playlist's track list (`ui/library/PlaylistEntries.kt`).
