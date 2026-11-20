# MacImportSheetCaption

A CUE sheet's caption: the place pop-up showing which side or disc the sheet is on (`disc`), named by its heading: Side A, Side B on a record or cassette, Disc 1 elsewhere, CUE, the sheet's name as a button that opens it in the viewer, → and the Audio file pop-up showing the file its FILE line is bound to (“Choose audio…” in warning while unbound). A library release's editor draws the same caption: its place pop-up offers every side and disc of the release, with no Ignored, and its Audio file pop-up offers the release's audio files on this device, with no Choose audio…, since the editor never adds or removes tracks.

**In the code:** macOS: `MacImportSheetCaption` (`Views/Components/MacImportSheetCaption.swift`), which takes the place and the audio as views. The import pane and the release editor both give them in `ReleaseSheetCaption` (`Views/Components/MetadataForm/ReleaseSheetCaption.swift`), from the one sheet caption core hands both.
