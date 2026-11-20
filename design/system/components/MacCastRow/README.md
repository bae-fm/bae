# MacCastRow

A receiver, wherever bae lists them: the Play on popover and Settings → Playback. Its kind's icon, its name, then its kind under it (Chromecast, AirPlay, UPnP). The receiver playing (`active`) says "Playing" in the accent instead and ends in Disconnect, a small borderless `MacButton` in primary text; clicking any other row plays on that receiver. Each receiver shows once, and at most one is ever playing, since playback goes to one output at a time. A long name wraps to two lines, then ends in an ellipsis.

In Settings → Playback (`menu`) every row ends in a ⋯ button instead, as Welcome's library rows do: Details…, a popover with what bae knows of the receiver, and, on the one playing, Disconnect. The Play on popover keeps Disconnect on the row, since stopping is what you open it for.

On Android the same row is `AndroidCastRow`.

**In the code:** macOS: `CastReceiverRow` (`Views/Playback/NowPlaying/CastReceiverRow.swift`), in `CastButton`'s popover and the Playback settings tab.
