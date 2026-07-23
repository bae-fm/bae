package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import fm.bae.app.BridgeFixtures
import fm.bae.app.data.Library
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerWorkGroup
import uniffi.bae_bridge.BridgeMetadataSource
import uniffi.bae_bridge.BridgeReleaseRoleSummary
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.BridgeWorkReleaseSummary

/**
 * The media-browse tree ([LibraryBrowseTree]) built over the real [Library]
 * projection and a fake bridge handle. Exercises the tree shape, media-id
 * encoding on each node, playable-vs-browsable flags, the release-wide track
 * index tracks carry, and that the requested page window reaches the paged
 * bridge reads unaltered.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LibraryBrowseTreeTest {
    private val labels = BrowseLabels(albums = "Albums", composers = "Composers")

    private fun tree(handle: FakeAppHandle): LibraryBrowseTree =
        LibraryBrowseTree(
            library = Library(handle),
            labels = { labels },
            artworkUri = { coverId -> Uri.parse("content://test/cover/$coverId") },
        )

    private fun track(
        id: String,
        title: String,
    ): BridgeTrack =
        BridgeTrack(
            id = id,
            title = title,
            side = 0,
            trackNumber = null,
            durationMs = 180_000L,
            durationClock = null,
            artistNames = "Artist Name",
            displayArtist = null,
            positionText = "1",
        )

    private val MediaItem.title: String?
        get() = mediaMetadata.title?.toString()

    @Test
    fun rootIsBrowsableWithTheRootId() {
        val root = tree(FakeAppHandle()).root()
        assertEquals(BrowseId.Root.mediaId, root.mediaId)
        assertTrue(root.mediaMetadata.isBrowsable == true)
        assertFalse(root.mediaMetadata.isPlayable == true)
    }

    @Test
    fun rootChildrenAreTheTwoCategories() =
        runBlocking {
            val children = tree(FakeAppHandle()).children(BrowseId.Root.mediaId, page = 0, pageSize = 10)!!
            assertEquals(listOf(BrowseId.Albums.mediaId, BrowseId.Composers.mediaId), children.map { it.mediaId })
            assertEquals(listOf("Albums", "Composers"), children.map { it.title })
            assertTrue(children.all { it.mediaMetadata.isBrowsable == true })
        }

    @Test
    fun albumsCategoryPagesAlbumsAndHonorsTheWindow() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    albumPages = { _, _ -> listOf(BridgeFixtures.album(id = "album-1", title = "First Album")) },
                )
            val children = tree(handle).children(BrowseId.Albums.mediaId, page = 2, pageSize = 20)!!

            assertEquals(listOf(BrowseId.Album("album-1").mediaId), children.map { it.mediaId })
            assertEquals(listOf("First Album"), children.map { it.title })
            assertTrue(children.single().mediaMetadata.isBrowsable == true)
            // page 2 × pageSize 20 → offset 40, limit 20, straight to the bridge.
            assertEquals(40uL to 20uL, handle.albumPageWindows.single())
        }

    @Test
    fun albumDrillsToItsPrimaryReleaseTracksWithFlatIndices() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-1")
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Sided("A"),
                                headerKey = "core.track.side",
                                tracks = listOf(track("t0", "Opener"), track("t1", "Second")),
                            ),
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Sided("B"),
                                headerKey = "core.track.side",
                                tracks = listOf(track("t2", "Third")),
                            ),
                        ),
                )
            val handle = FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(release))))

            val children = tree(handle).children(BrowseId.Album("album-1").mediaId, page = 0, pageSize = 10)!!

            assertEquals(
                listOf(
                    BrowseId.Track("rel-1", 0).mediaId,
                    BrowseId.Track("rel-1", 1).mediaId,
                    BrowseId.Track("rel-1", 2).mediaId,
                ),
                children.map { it.mediaId },
            )
            assertEquals(listOf("Opener", "Second", "Third"), children.map { it.title })
            assertTrue(children.all { it.mediaMetadata.isPlayable == true })
            assertTrue(children.none { it.mediaMetadata.isBrowsable == true })
        }

    @Test
    fun trackChildPageIsSliced() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-1")
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Flat,
                                headerKey = null,
                                tracks = listOf(track("t0", "A"), track("t1", "B"), track("t2", "C")),
                            ),
                        ),
                )
            val handle = FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(release))))

            val children = tree(handle).children(BrowseId.Album("album-1").mediaId, page = 1, pageSize = 2)!!

            // page 1 × 2 → skip 2, take 2 → only the third track survives.
            assertEquals(listOf(BrowseId.Track("rel-1", 2).mediaId), children.map { it.mediaId })
        }

    @Test
    fun composersCategoryPagesComposers() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    composerPages = { _, _ -> listOf(BridgeFixtures.composerSummary(artistId = "artist-1", name = "A Composer")) },
                )
            val children = tree(handle).children(BrowseId.Composers.mediaId, page = 0, pageSize = 50)!!

            assertEquals(listOf(BrowseId.Composer("artist-1").mediaId), children.map { it.mediaId })
            assertEquals(listOf("A Composer"), children.map { it.title })
            assertEquals(0uL to 50uL, handle.composerPageWindows.single())
        }

    @Test
    fun composerDrillsToWorksThenCreditedAlbums() =
        runBlocking {
            val detail =
                BridgeComposerDetail(
                    composer = BridgeFixtures.composerSummary(artistId = "artist-1"),
                    workGroups =
                        listOf(
                            BridgeComposerWorkGroup(
                                id = "group-1",
                                parent = null,
                                works = listOf(BridgeFixtures.workSummary(workId = "work-1", title = "A Work")),
                            ),
                        ),
                    unlinkedReleaseRoles =
                        listOf(
                            BridgeReleaseRoleSummary(
                                releaseId = "rel-9",
                                albumId = "album-9",
                                albumTitle = "Credited Album",
                                source = BridgeMetadataSource.MUSIC_BRAINZ,
                                sourceCredit = null,
                            ),
                        ),
                    unlinkedTrackRoles = emptyList(),
                    defaultWorkId = null,
                )
            val handle = FakeAppHandle(composerDetails = mapOf("artist-1" to detail))

            val children = tree(handle).children(BrowseId.Composer("artist-1").mediaId, page = 0, pageSize = 10)!!

            assertEquals(
                listOf(BrowseId.Work("work-1").mediaId, BrowseId.Album("album-9").mediaId),
                children.map { it.mediaId },
            )
            assertEquals(listOf("A Work", "Credited Album"), children.map { it.title })
        }

    @Test
    fun workDrillsToChildWorksThenReleases() =
        runBlocking {
            val detail =
                BridgeWorkDetail(
                    work = BridgeFixtures.workSummary(workId = "work-1"),
                    childWorks = listOf(BridgeFixtures.workSummary(workId = "work-2", title = "Child Work")),
                    releases =
                        listOf(
                            BridgeWorkReleaseSummary(
                                releaseId = "rel-3",
                                albumId = "album-3",
                                albumTitle = "Work Album",
                                displayName = "Release",
                                format = null,
                                cover = null,
                            ),
                        ),
                    tracks = emptyList(),
                )
            val handle = FakeAppHandle(workDetails = mapOf("work-1" to detail))

            val children = tree(handle).children(BrowseId.Work("work-1").mediaId, page = 0, pageSize = 10)!!

            assertEquals(
                listOf(BrowseId.Work("work-2").mediaId, BrowseId.Album("album-3").mediaId),
                children.map { it.mediaId },
            )
        }

    @Test
    fun unknownParentReturnsNull() =
        runBlocking {
            assertNull(tree(FakeAppHandle()).children("nonsense", page = 0, pageSize = 10))
        }

    @Test
    fun searchReturnsBrowsableAlbumResults() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(
                            albums = listOf(BridgeFixtures.albumSearchResult(id = "album-7", title = "Found Album")),
                        )
                    },
                )
            val results = tree(handle).search("query", page = 0, pageSize = 10)

            assertEquals(listOf(BrowseId.Album("album-7").mediaId), results.map { it.mediaId })
            assertEquals(listOf("Found Album"), results.map { it.title })
            assertTrue(results.single().mediaMetadata.isBrowsable == true)
        }

    @Test
    fun spokenQueryResolvesToATrackInItsPrimaryRelease() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-1")
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Flat,
                                headerKey = null,
                                tracks = listOf(track("t0", "A"), track("t1", "B")),
                            ),
                        ),
                )
            val handle =
                FakeAppHandle(
                    albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(release))),
                    searchResults = {
                        BridgeFixtures.searchResults(
                            tracks = listOf(BridgeFixtures.trackSearchResult(id = "t1", albumId = "album-1")),
                        )
                    },
                )

            assertEquals(BrowseId.Track("rel-1", 1), tree(handle).searchTopPlayable("query"))
        }

    @Test
    fun spokenQueryWithNoResultsResolvesToNull() =
        runBlocking {
            assertNull(tree(FakeAppHandle()).searchTopPlayable("query"))
        }

    @Test
    fun itemResolvesATrackByReleaseAndIndex() =
        runBlocking {
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Flat,
                                headerKey = null,
                                tracks = listOf(track("t0", "A"), track("t1", "B")),
                            ),
                        ),
                )
            val handle = FakeAppHandle(releaseDetails = mapOf("rel-1" to release))

            val item = tree(handle).item(BrowseId.Track("rel-1", 1).mediaId)!!

            assertEquals(BrowseId.Track("rel-1", 1).mediaId, item.mediaId)
            assertEquals("B", item.title)
            assertTrue(item.mediaMetadata.isPlayable == true)
        }
}
