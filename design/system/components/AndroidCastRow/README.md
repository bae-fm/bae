# AndroidCastRow

A receiver, in the Play on picker and in the Playback settings alike: its kind's icon, its name, and its kind under it (Chromecast, AirPlay, UPnP); or a wait ("Looking for devices…", "Connecting…"). Tapping a receiver plays on it. The receiver playing (`active`) is the same row: "Playing" under its name in the accent, and Disconnect at the right. There is no separate "Playing on" row and no check, so each receiver shows once. A long name wraps to two lines, then ends in an ellipsis.

In the Playback settings (`menu`) every row ends in ⋮ instead, so the group reads as a list of receivers, not of actions: the menu holds Details…, a sheet with what bae knows of the receiver, and, on the one playing, Disconnect. The picker keeps Disconnect on the row, since stopping is what you open it for.

**In the code:** Android: `AndroidCastRow` (`ui/playback/AndroidCastRow.kt`), in the Play on picker and the Playback settings.
