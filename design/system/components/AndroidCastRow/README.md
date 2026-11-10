# AndroidCastRow

A line of the cast picker: a receiver (its kind's icon and name), or a wait ("Looking for devices…", "Connecting…"). The receiver being cast to (`active`) is the same row: "Casting" under its name in the accent, and Disconnect at the right. There is no separate "Casting to" row and no check, so each receiver shows once. A long name wraps to two lines, then ends in an ellipsis.

**In the code:** Android: the picker rows in `CastButton` (`ui/playback/CastButton.kt`).
