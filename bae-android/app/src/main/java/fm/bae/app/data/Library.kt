package fm.bae.app.data

import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.map
import fm.bae.screensubscription.pulled
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumDetailRequest
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeHistoryPlay
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeLibrarySearchSnapshot
import uniffi.bae_bridge.BridgeList
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgePlaylistDetail
import uniffi.bae_bridge.BridgePlaylistListing
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.LibrarySearchSubscription

/**
 * One value a detail read delivered: the id it was read for, and that item's detail, or none once
 * no such item exists.
 */
internal data class DetailDelivery<Id, Value>(
    val id: Id?,
    val value: Value?,
)

/**
 * A detail view's live read: the id it shows changes in place, and each value it is pulled for
 * names the id it answers.
 */
internal interface DetailRead<Id, Value> : PulledSubscription<DetailDelivery<Id, Value>> {
    fun setId(id: Id)

    /** Free the read without waiting, from a scope that is already ending. */
    fun release()
}

private class BridgeDetailRead<Id, Value>(
    private val set: (Id) -> Unit,
    pulls: PulledSubscription<DetailDelivery<Id, Value>>,
    private val free: () -> Unit,
) : DetailRead<Id, Value>,
    PulledSubscription<DetailDelivery<Id, Value>> by pulls {
    override fun setId(id: Id) = set(id)

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
    /** The album grid under one sort and grouping: its view and count, each a live read of its own. */
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
            rows = ::albumRows,
        )

    /** The reads of [list], taking its rows out of the variant of [BridgeListRows] they come in. */
    internal fun <Row> listReads(
        list: BridgeList,
        sections: (() -> PulledSubscription<List<BridgeLibraryBrowseSection>>)? = null,
        rows: (BridgeListRows) -> List<Row>,
    ): ListReads<Row> =
        ListReads(
            view = { first, last ->
                handle.subscribeListView(list, first.toULong(), last.toULong()).let { subscription ->
                    val pulls =
                        pulled(subscription::next, subscription::cancel).map {
                            ListViewDelivery(it.start.toInt(), rows(it.rows))
                        }
                    object : ListViewRead<Row>, PulledSubscription<ListViewDelivery<Row>> by pulls {
                        override fun setVisible(
                            first: Int,
                            last: Int,
                        ) = subscription.setVisible(first.toULong(), last.toULong())
                    }
                }
            },
            count = { handle.subscribeListCount(list).let { pulled(it::next, it::cancel) } },
            sections = sections,
        )

    /**
     * Each detail view keeps one live read, and the view sets that read's id to each item it
     * shows. The album page's read takes the album and the release the person chose on the page.
     */
    internal fun albumDetail(): DetailRead<BridgeAlbumDetailRequest, BridgeAlbumDetail> =
        handle.subscribeAlbumDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun artistDetail(): DetailRead<String, BridgeArtistDetail> =
        handle.subscribeArtistDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun composerDetail(): DetailRead<String, BridgeComposerDetail> =
        handle.subscribeComposerDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun workDetail(): DetailRead<String, BridgeWorkDetail> =
        handle.subscribeWorkDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun playlistDetail(): DetailRead<String, BridgePlaylistDetail> =
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
        listReads(BridgeList.Composers(listOf(sortCriterion)), rows = ::composerRows)

    /** One composer's page rows. */
    internal fun composerPageList(artistId: String): ListReads<BridgeComposerDetailRow> =
        listReads(BridgeList.ComposerPage(artistId), rows = ::composerPageRows)

    /** The artist list under one sort. */
    internal fun artistList(sortCriterion: BridgeArtistSortCriterion): ListReads<BridgeArtistSummary> =
        listReads(BridgeList.Artists(listOf(sortCriterion))) { rows ->
            (rows as? BridgeListRows.Artists)?.rows ?: error("the artist list delivered rows of another list")
        }

    /** History's plays, newest first: everyone's with a null [person], else those of the person with that member id. */
    internal fun historyList(person: String?): ListReads<BridgeHistoryPlay> =
        listReads(BridgeList.History(person)) { rows ->
            (rows as? BridgeListRows.History)?.rows ?: error("History delivered rows of another list")
        }

    /** The download queue as the Downloads screen draws it, in the order queued. */
    internal fun downloadList(): ListReads<BridgeDownloadOp> =
        listReads(BridgeList.Downloads) { rows ->
            (rows as? BridgeListRows.Downloads)?.rows ?: error("the download queue delivered rows of another list")
        }

    /** One live library search, whose text changes on the same live query as the person types. */
    internal fun librarySearch(): LibrarySearch = BridgeLibrarySearch(handle.subscribeLibrarySearch())
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
