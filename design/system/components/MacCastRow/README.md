# MacCastRow

A receiver, wherever bae lists them: the cast popover and Settings → Casting. Its kind's icon, its name, then its kind under it (Chromecast, AirPlay, UPnP). The receiver being cast to (`active`) says "Casting" in the accent instead and ends in Disconnect; clicking any other row casts to that receiver. Each receiver shows once, and at most one is ever casting, since playback goes to one output at a time. A long name wraps to two lines, then ends in an ellipsis.

On Android the same row is `AndroidCastRow`.

**In the code:** macOS: the rows of `CastButton`'s popover (`Views/Playback/NowPlaying/CastButton.swift`) and the Casting settings pane; one view for both.
