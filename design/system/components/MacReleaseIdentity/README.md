# MacReleaseIdentity

The release's cover beside its identity, at the top of the import draft and the release editor's card.

- **Cover:** `cover`, or a plain tile when the release has none.
- **Album title** (`title`) and **album year** (`year`): `MacInlineField`s, the title large and the year quiet.
- **Album artist:** a `MacAlbumArtistField` (`artist`, with `artistChip`), in secondary text.
- **Children:** what the card puts under the year, in order: the folder's audio line and the `MacAlbumPicker`.

**In the code:** macOS: `ReleaseAlbumIdentityEditor` (`Views/Components/MetadataForm/ReleaseMetadataHeader.swift`), whose callers pass the audio line (`SourceAudioLine`) and the album row (`DraftAlbumRow`).
