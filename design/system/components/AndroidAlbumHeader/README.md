# AndroidAlbumHeader

Tops the album page: the 150pt cover (opens the gallery when there is one), then the title, the artist and the album's year, each on its own line, then the selected release on two lines when known. The edition line (`edition`): the release's own year, even when it matches the album's, its country in the short form the person's language commonly uses, then each label with its catalog numbers. The copy line (`copy`): the media and details, the audio and the length. Core words both lines, and Mac shows the same words. The release lines draw in the tertiary text colour, a step lighter than the artist and year above them.

**In the code:** Android: `AndroidAlbumHeader` in `ui/albumdetail/AlbumDetailScreen.kt`.
