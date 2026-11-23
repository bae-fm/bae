# MacCastRow

A receiver, wherever bae lists them: the Play on popover and Settings → Playback. Its kind's icon, its name, then its kind under it (Chromecast, AirPlay, UPnP). The receiver playing (`active`) says "Playing" in the accent instead and ends in Disconnect, a small borderless `MacButton` in the accent; clicking any other row plays on that receiver. The icon, the spinner, Disconnect and the ⋯ sit at the middle of the row's two lines, and the name starts 10px after the icon. Each receiver shows once, and at most one is ever playing, since playback goes to one output at a time. A long name wraps to two lines, then ends in an ellipsis.

While bae connects to the receiver, or waits for its owner to accept the connection (`waiting`, the words), the row shows a small spinner at its end, where Disconnect sits on the row of the receiver playing, and says the words under the name in italic secondary text in place of its kind: "Connecting…" or "Accept on Kitchen…". On a row that is one button, VoiceOver reads the wait in the button's name and reads no spinner.

In Settings → Playback (`menu`) every row ends in a ⋯ button instead, after the spinner on a waiting row, as Welcome's library rows do: Details…, a popover with what bae knows of the receiver, and, on the one playing, Disconnect. The Play on popover keeps Disconnect on the row, since stopping is what you open it for.

On Android the same row is `AndroidCastRow`.

**In the code:** macOS: `MacCastRow` (`Views/Playback/NowPlaying/MacCastRow.swift`), in `CastButton`'s popover and the Playback settings tab.
