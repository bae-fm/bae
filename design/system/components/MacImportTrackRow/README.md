# MacImportTrackRow

A track row: the Play button (Stop while `playing`) and its source, position, the title as a field, the artist, length. An empty title shows Untitled. The artist is a `MacAlbumArtistField` that fills its column (`artist`, with `artistChip`): one the track inherits from the album (`inherited`) shows Album artist, and a track with no artist shows Artist, each in the tertiary colour. The source is the file, or for a CUE image's track (`start`) where the track starts in the image, which the sheet's caption names once; the entry's title is in the title column only. The length is what the audio measures; where the source lists one more than 3 s apart (`listed`), that follows it in the warning colour. `playing` tints the row. The import pane and the library's Edit release sheet draw the same row, and both play.

The title is a `MacInlineField`.

**In the code:** macOS: `MacImportTrackRow` and `TrackSourceCell` (`Views/Import/Mapping/`) and `ReleaseMetadataTrackRow` (`Views/Components/MetadataForm/`).

No file, track or image carries a label saying which clue was read from it (a Disc ID from the rip log, a barcode from an image): those labels are removed from the import everywhere.
