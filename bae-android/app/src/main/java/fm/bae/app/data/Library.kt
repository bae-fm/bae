package fm.bae.app.data

import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.map
import fm.bae.screensubscription.pulled
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeBrowseTrack
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeLibrarySearchSnapshot
import uniffi.bae_bridge.BridgeList
import uniffi.bae_bridge.BridgeListPosition
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgeListWindow
import uniffi.bae_bridge.BridgePlaylistDetail
import uniffi.bae_bridge.BridgePlaylistListing
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortValue
import uniffi.bae_bridge.BridgeWorkChild
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.LibrarySearchSubscription

/**
 * One value a detail read delivered: the id it was read for, and that item's detail, or none once
 * no such item exists.
 */
internal data class DetailDelivery<Value>(
    val id: String?,
    val value: Value?,
)

/**
 * A detail view's live read: the id it shows changes in place, and each value it is pulled for
 * names the id it answers.
 */
internal interface DetailRead<Value> : PulledSubscription<DetailDelivery<Value>> {
    fun setId(id: String)

    /** Free the read without waiting, from a scope that is already ending. */
    fun release()
}

private class BridgeDetailRead<Value>(
    private val set: (String) -> Unit,
    pulls: PulledSubscription<DetailDelivery<Value>>,
    private val free: () -> Unit,
) : DetailRead<Value>,
    PulledSubscription<DetailDelivery<Value>> by pulls {
    override fun setId(id: String) = set(id)

    override fun release() = free()
}

/**
 * Narrow projection of [AppHandleInterface] for library browse and detail live queries.
 * Image bytes are not here: every image in the app resolves through
 * [fm.bae.app.data.ImageStore], which owns their caching too.
 * Mirrors the macOS `Library` domain service.
 */
class Library(
    private val handle: AppHandleInterface,
) {
    /** The album grid under one sort and grouping: its windows, count and positions, each a live read of its own. */
    internal fun albumList(
        sortCriteria: List<BridgeSortCriterion>,
        groupByArtist: Boolean = false,
    ): ListReads<BridgeAlbumAppearance> =
        listReads(
            BridgeList.AlbumGrid(sortCriteria, groupByArtist),
            sections =
                if (groupByArtist) {
                    {
                        handle.subscribeAlbumSections(sortCriteria).let { pulled(it::next, it::cancel) }
                    }
                } else {
                    null
                },
        ) { rows ->
            (rows as? BridgeListRows.Albums)?.rows ?: error("the album grid delivered rows of another list")
        }

    /** The reads of [list], taking its rows out of the variant of [BridgeListRows] they come in. */
    internal fun <Row> listReads(
        list: BridgeList,
        sections: (() -> PulledSubscription<List<BridgeLibraryBrowseSection>>)? = null,
        rows: (BridgeListRows) -> List<Row>,
    ): ListReads<Row> =
        ListReads(
            window = { window ->
                handle.subscribeListWindow(list, window).let { subscription ->
                    val pulls =
                        pulled(subscription::next, subscription::cancel).map {
                            ListWindowDelivery(it.window, rows(it.rows), it.first, it.beyond)
                        }
                    object : ListWindowRead<Row>, PulledSubscription<ListWindowDelivery<Row>> by pulls {
                        override fun setWindow(window: BridgeListWindow) = subscription.setWindow(window)
                    }
                }
            },
            count = { handle.subscribeListCount(list).let { pulled(it::next, it::cancel) } },
            position = { key ->
                handle.subscribeListPosition(list, key).let { subscription ->
                    val pulls = pulled(subscription::next, subscription::cancel)
                    object : ListPositionRead, PulledSubscription<BridgeListPosition> by pulls {
                        override fun setKey(key: List<BridgeSortValue>) = subscription.setKey(key)
                    }
                }
            },
            keyAt = { position -> handle.listKeyAt(list, position) },
            sections = sections,
        )

    /** A detail view's live reads, each moved to every item its view shows in place. */
    internal fun albumDetail(): DetailRead<BridgeAlbumDetail> =
        handle.subscribeAlbumDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun releaseDetail(): DetailRead<BridgeRelease> =
        handle.subscribeReleaseDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun artistDetail(): DetailRead<BridgeArtistDetail> =
        handle.subscribeArtistDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun composerDetail(): DetailRead<BridgeComposerDetail> =
        handle.subscribeComposerDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun workDetail(): DetailRead<BridgeWorkDetail> =
        handle.subscribeWorkDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun playlistDetail(): DetailRead<BridgePlaylistDetail> =
        handle.subscribePlaylistDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    /** Every playlist this device holds, now and after each change. */
    internal fun playlists(): PulledSubscription<BridgePlaylistListing> {
        val read = handle.subscribePlaylists()
        return pulled(read::next, read::cancel)
    }

    /** The composer list under one sort. */
    internal fun composerList(sortCriterion: BridgeComposerSortCriterion): ListReads<BridgeComposerSummary> =
        listReads(BridgeList.Composers(listOf(sortCriterion))) { rows ->
            (rows as? BridgeListRows.Composers)?.rows ?: error("the composer list delivered rows of another list")
        }

    /** One composer's page rows. */
    internal fun composerPageList(artistId: String): ListReads<BridgeComposerDetailRow> =
        listReads(BridgeList.ComposerPage(artistId)) { rows ->
            (rows as? BridgeListRows.ComposerPage)?.rows ?: error("a composer's page delivered rows of another list")
        }

    /** The artist list under one sort. */
    internal fun artistList(sortCriterion: BridgeArtistSortCriterion): ListReads<BridgeArtistSummary> =
        listReads(BridgeList.Artists(listOf(sortCriterion))) { rows ->
            (rows as? BridgeListRows.Artists)?.rows ?: error("the artist list delivered rows of another list")
        }

    /** The download queue as the Downloads screen draws it, in the order queued. */
    internal fun downloadList(): ListReads<BridgeDownloadOp> =
        listReads(BridgeList.Downloads) { rows ->
            (rows as? BridgeListRows.Downloads)?.rows ?: error("the download queue delivered rows of another list")
        }

    /** One live library search, whose text changes on the same live query as the person types. */
    internal fun librarySearch(): LibrarySearch = BridgeLibrarySearch(handle.subscribeLibrarySearch())

    /** The albums library search lists for [query] as a media browser lists them, in title order. */
    internal fun searchAlbumList(query: String): ListReads<BridgeAlbumSearchResult> =
        listReads(BridgeList.SearchAlbums(query)) { rows ->
            (rows as? BridgeListRows.SearchAlbums)?.rows ?: error("a search's albums delivered rows of another list")
        }

    /** The album [albumId]'s tracks as a media browser lists them: its main release's, in playing order. */
    internal fun albumTrackList(albumId: String): ListReads<BridgeBrowseTrack> =
        listReads(BridgeList.AlbumTracks(albumId)) { rows ->
            (rows as? BridgeListRows.AlbumTracks)?.rows ?: error("an album's tracks delivered rows of another list")
        }

    /** The work [workId]'s children as a media browser lists them: its parts, then the releases that perform it. */
    internal fun workChildList(workId: String): ListReads<BridgeWorkChild> =
        listReads(BridgeList.WorkChildren(workId)) { rows ->
            (rows as? BridgeListRows.WorkChildren)?.rows ?: error("a work's children delivered rows of another list")
        }
}

/** A live library search: point it at a query, and each value it is pulled for names the query it answers. */
internal interface LibrarySearch : PulledSubscription<BridgeLibrarySearchSnapshot> {
    /**
     * Search for [query] from now on, or for nothing when it is null; returns the request revision
     * its answers will carry.
     */
    fun setQuery(query: String?): ULong

    /** Free the search without waiting, from a scope that is already ending. */
    fun release()
}

private class BridgeLibrarySearch(
    private val subscription: LibrarySearchSubscription,
) : LibrarySearch,
    PulledSubscription<BridgeLibrarySearchSnapshot> by pulled(subscription::next, subscription::cancel) {
    override fun setQuery(query: String?): ULong = subscription.setQuery(query)

    override fun release() = subscription.close()
}
