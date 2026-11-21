# MacAlbumPicker

The album the release files under: Album, then a pop-up showing the album as it stands (`album`): New album, the library album the release's records name, or one the person picked.

`open` draws its menu: the album as it stands first, then New album when that album is one the library has (`inLibrary`), Use the release’s album when the person picked another album (`picked`), a line, and Choose another album…, which searches the library's albums. `off` turns the pop-up off, as the editor does while a save reads a file: a pick then would cancel the save.

**In the code:** macOS: `MacAlbumPicker` (`Views/Components/MetadataForm/MacAlbumPicker.swift`).
