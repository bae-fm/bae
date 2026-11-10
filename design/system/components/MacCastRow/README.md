# MacCastRow

A receiver, wherever bae lists them: the cast popover and Settings → Casting. Its kind's icon, its name, then its kind under it (Chromecast, AirPlay, UPnP). The receiver being cast to (`active`) says "Casting" in the accent instead and ends in Disconnect; clicking any other row casts to that receiver. Each receiver shows once, and at most one is ever casting, since playback goes to one output at a time. A long name wraps to two lines, then ends in an ellipsis.

In Settings → Casting (`menu`) every row ends in a ⋯ button instead, as Welcome's library rows do: Details…, a popover with what bae knows of the receiver, and, on the one being cast to, Disconnect. The cast popover keeps Disconnect on the row, since stopping is what you open it for.

On Android the same row is `AndroidCastRow`.

**In the code:** macOS: `CastReceiverRow` (`Views/Playback/NowPlaying/CastReceiverRow.swift`), in `CastButton`'s popover and the Casting settings tab.
