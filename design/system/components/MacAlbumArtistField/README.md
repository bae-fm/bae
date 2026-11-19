# MacAlbumArtistField

An artist field: a button that opens the artist picker, drawn as its value until the pointer is over it or it has focus, when it shows the field's box, as a `MacInlineField` does.

| Prop | What it does |
|---|---|
| `artist` | The artists the field names. |
| `chip` | Library for an artist the library has, New for one the import adds. |
| `placeholder` | Shown in the tertiary colour while the field names no one: Album artist unless given (a track row's says Artist). |
| `inherited` | A track that takes the album's artists: Album artist in the tertiary colour. |
| `secondary` | The artists in secondary text, as the album identity draws them. |
| `column` | Fills its column, as a track row's artist does; otherwise the field is as wide as its value. |

**In the code:** macOS: `ArtistAssignmentsField` (`Views/Components/MetadataForm/ReleaseMetadataHeader.swift`), which owns its `FieldChrome` and its text style, in `ReleaseAlbumIdentityEditor` and `ReleaseMetadataTrackRow`.
