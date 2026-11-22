# MacQueueSection

A lane's heading: "Up next" (only when it has tracks), "Playing from · <album>", "Your Library" or "These tracks"; Clear when it has tracks; shuffle on the Playing from lane, accent while on. `menu` ends History's first day heading in whose plays it shows: the current choice in the accent with a ⌄, which opens a menu of the choices.

`clear` names the Clear button for VoiceOver.

`hang`: in a pane whose rows hang their leading column (`MacQueuePanel`'s `hang`), the heading starts at the rows' covers.

**In the code:** macOS: `MacQueueSection` (`Views/Playback/Queue/MacQueueSection.swift`), the heading `QueueSection` draws over its rows.
