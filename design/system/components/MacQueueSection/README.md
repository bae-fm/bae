# MacQueueSection

A lane's heading: "Up next" (only when it has tracks), "Playing from · <album>", "Your Library" or "These tracks"; Clear when it has tracks; shuffle on the Playing from lane, accent while on. `menu` ends History's first day heading in whose plays it shows: the current choice in the accent with a ⌄, which opens a menu of the choices.

`clear` names the Clear button for VoiceOver.

`day`: a day of History, in a pane whose rows hang their times in the margin (`MacQueuePanel`'s `hang`): the heading starts at the rows' covers and ends at the pane's edge, and only the first day takes `menu`. A lane takes `clear` and `shuffle`, and a day takes neither.

**In the code:** macOS: `MacQueueSection` (`Views/Playback/Queue/MacQueueSection.swift`), the heading `QueueSection` draws over its rows.
