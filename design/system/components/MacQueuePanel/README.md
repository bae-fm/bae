# MacQueuePanel

The queue docked at the window's right (420pt): Queue and Close, the Now playing card while something plays, then the sections. `empty`: "Queue is empty" / "Drag tracks here or play an album". Put it as a child of `MacMainWindow`.

**In the code:** macOS: `MacQueuePanel` (`Views/Playback/Queue/MacQueuePanel.swift`) draws the panel from the lanes and now-playing it is given; `MacQueuePanelContainer` reads the stores for it.
