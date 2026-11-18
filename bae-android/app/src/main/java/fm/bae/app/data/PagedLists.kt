package fm.bae.app.data

import fm.bae.screensubscription.map
import fm.bae.screensubscription.pulled
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeBrowseTrack
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeList
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgePagedList
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeWorkChild

/**
 * The library's lists as a media browser reads them, a page at a time: each page a live read of the rows at its
 * places, and each list's count.
 */
internal class PagedLists(
    private val handle: AppHandleInterface,
) {
    /**
     * The reads of [list] a media browser shows a page at a time, which [counted] counts, taking each page's rows
     * out of the variant of [BridgeListRows] they come in.
     */
    private fun <Row> pageReads(
        list: BridgePagedList,
        counted: BridgeList,
        rows: (BridgeListRows) -> List<Row>,
    ): PageReads<Row> =
        PageReads(
            page = { offset, limit ->
                handle.subscribeListPage(list, offset.toULong(), limit.toULong()).let {
                    pulled(it::next, it::cancel).map(rows)
                }
            },
            count = { handle.subscribeListCount(counted).let { pulled(it::next, it::cancel) } },
        )

    /** The album grid under one sort, flat, a page at a time. */
    internal fun albumPages(sortCriteria: List<BridgeSortCriterion>): PageReads<BridgeAlbumAppearance> =
        pageReads(BridgePagedList.AlbumGrid(sortCriteria, false), BridgeList.AlbumGrid(sortCriteria, false)) { rows ->
            (rows as? BridgeListRows.Albums)?.rows ?: error("the album grid delivered rows of another list")
        }

    /** The composer list under one sort, a page at a time. */
    internal fun composerPages(sortCriterion: BridgeComposerSortCriterion): PageReads<BridgeComposerSummary> =
        pageReads(
            BridgePagedList.Composers(listOf(sortCriterion)),
            BridgeList.Composers(listOf(sortCriterion)),
        ) { rows ->
            (rows as? BridgeListRows.Composers)?.rows ?: error("the composer list delivered rows of another list")
        }

    /** One composer's page rows, a page at a time. */
    internal fun composerPagePages(artistId: String): PageReads<BridgeComposerDetailRow> =
        pageReads(BridgePagedList.ComposerPage(artistId), BridgeList.ComposerPage(artistId)) { rows ->
            (rows as? BridgeListRows.ComposerPage)?.rows ?: error("a composer's page delivered rows of another list")
        }

    /** The albums library search lists for [query] as a media browser lists them, in title order. */
    internal fun searchAlbumPages(query: String): PageReads<BridgeAlbumSearchResult> =
        pageReads(BridgePagedList.SearchAlbums(query), BridgeList.SearchAlbums(query)) { rows ->
            (rows as? BridgeListRows.SearchAlbums)?.rows ?: error("a search's albums delivered rows of another list")
        }

    /** The album [albumId]'s tracks as a media browser lists them: its main release's, in playing order. */
    internal fun albumTrackPages(albumId: String): PageReads<BridgeBrowseTrack> =
        pageReads(BridgePagedList.AlbumTracks(albumId), BridgeList.AlbumTracks(albumId)) { rows ->
            (rows as? BridgeListRows.AlbumTracks)?.rows ?: error("an album's tracks delivered rows of another list")
        }

    /** The work [workId]'s children as a media browser lists them: its parts, then the releases that perform it. */
    internal fun workChildPages(workId: String): PageReads<BridgeWorkChild> =
        pageReads(BridgePagedList.WorkChildren(workId), BridgeList.WorkChildren(workId)) { rows ->
            (rows as? BridgeListRows.WorkChildren)?.rows ?: error("a work's children delivered rows of another list")
        }
}
