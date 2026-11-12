# MacProgress

The words say what is happening now, ending in an ellipsis: "Checking the folder “Music”…". Leave out `value` when the length isn't known.

**In the code:** macOS: `MacProgress` (`Views/Components/MacProgress.swift`), a step's name over a bar, drawn from the values it is given. `MacProgressContainer` follows the step streams of a library's open and of setup calls and draws the same look natively, and `ImportProgressContainer` follows an import's percent stream for the importing pane. The bar is drawn by `ProgressTrackDrawing` (`Views/Components/ProgressTrack.swift`); `ProgressTrackBar` for a bar alone.
