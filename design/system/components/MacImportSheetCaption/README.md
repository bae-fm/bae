# MacImportSheetCaption

A CUE sheet's caption: the Disc pop-up showing which disc the sheet is (`disc`), CUE, the sheet's name as a button that opens it in the viewer, → and the Audio file pop-up showing the file its FILE line is bound to (“Choose audio…” in warning while unbound). A library release's editor draws the same caption: its Disc pop-up offers Disc 1 to the release's last disc, with no Ignored, and its Audio file pop-up offers the release's audio files on this device, with no Choose audio…, since the editor never adds or removes tracks.

**In the code:** macOS: `MacImportSheetCaption` (`Views/Components/MacImportSheetCaption.swift`), which takes the disc and the audio as views. The import pane gives them in `ImportPaneSheetCaption` (`Views/Import/Mapping/ImportPaneSheetCaption.swift`), and the release editor in `ReleaseEditSheetCaption` (`Views/Components/MetadataForm/ReleaseEditSheetCaption.swift`).
