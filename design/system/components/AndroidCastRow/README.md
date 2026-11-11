# AndroidCastRow

A receiver, in the cast picker and on Settings → Casting alike: its kind's icon, its name, and its kind under it (Chromecast, AirPlay, UPnP); or a wait ("Looking for devices…", "Connecting…"). Tapping a receiver casts to it. The receiver being cast to (`active`) is the same row: "Casting" under its name in the accent, and Disconnect at the right. There is no separate "Casting to" row and no check, so each receiver shows once. A long name wraps to two lines, then ends in an ellipsis.

On the Casting page (`menu`) every row ends in ⋮ instead, so the page reads as a list of receivers, not of actions: the menu holds Details…, a sheet with what bae knows of the receiver, and, on the one being cast to, Disconnect. The picker keeps Disconnect on the row, since stopping is what you open it for.

**In the code:** Android: `AndroidCastRow` (`ui/playback/AndroidCastRow.kt`), in the cast picker and the Casting page.
