package fm.bae.app.playback

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The browse-tree media-id encoding ([BrowseId]). Every node's id must
 * round-trip through [BrowseId.parse] so the browse client's navigate/play
 * requests resolve back to the typed node — including ids whose payload (a
 * library id) contains the `:` the encoding delimits on.
 */
class BrowseIdTest {
    @Test
    fun fixedCategoryIdsRoundTrip() {
        assertEquals(BrowseId.Root, BrowseId.parse(BrowseId.Root.mediaId))
        assertEquals(BrowseId.Albums, BrowseId.parse(BrowseId.Albums.mediaId))
        assertEquals(BrowseId.Composers, BrowseId.parse(BrowseId.Composers.mediaId))
    }

    @Test
    fun albumComposerWorkIdsRoundTrip() {
        val album = BrowseId.Album("album-1")
        val composer = BrowseId.Composer("artist-1")
        val work = BrowseId.Work("work-1")
        assertEquals(album, BrowseId.parse(album.mediaId))
        assertEquals(composer, BrowseId.parse(composer.mediaId))
        assertEquals(work, BrowseId.parse(work.mediaId))
    }

    @Test
    fun trackIdRoundTripsIndexAndRelease() {
        val track = BrowseId.Track(releaseId = "rel-1", index = 7)
        assertEquals(track, BrowseId.parse(track.mediaId))
    }

    @Test
    fun idsWithColonsInPayloadRoundTrip() {
        // A library id is opaque; the encoding must survive a `:` in it.
        val album = BrowseId.Album("prefix:album:9")
        val track = BrowseId.Track(releaseId = "rel:a:b", index = 3)
        assertEquals(album, BrowseId.parse(album.mediaId))
        assertEquals(track, BrowseId.parse(track.mediaId))
    }

    @Test
    fun unknownIdParsesToNull() {
        assertNull(BrowseId.parse("nonsense"))
        assertNull(BrowseId.parse(""))
    }

    @Test
    fun malformedTrackIdParsesToNull() {
        // Non-numeric index, and a missing release id.
        assertNull(BrowseId.parse("track:notanumber:rel-1"))
        assertNull(BrowseId.parse("track:5"))
    }

    @Test
    fun albumsCategoryIsNotMistakenForAnAlbumNode() {
        // "albums" must resolve to the category, not Album(payload="s").
        assertEquals(BrowseId.Albums, BrowseId.parse("albums"))
    }
}
