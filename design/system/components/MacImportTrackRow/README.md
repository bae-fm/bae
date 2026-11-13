# MacImportTrackRow

A track row: play/stop and its source, position, title, artist (“Album artist” when inherited), length. The source is the file, or for a CUE image's track (`start`) where the track starts in the image, which the sheet's caption names once; the entry's title is in the title column only. The length is what the audio measures; where the source lists one more than 3 s apart (`listed`), that follows it in the warning colour. `playing` tints the row. The import pane and the library's Edit release sheet draw the same row, and both play.

**In the code:** macOS: `MacImportTrackRow` and `ImportMappingSourceCell`.

No file, track or image carries a label saying which clue was read from it (a Disc ID from the rip log, a barcode from an image): those labels are removed from the import everywhere.
