package fm.bae.app.playback

import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import fm.bae.app.BaeLogger
import fm.bae.app.data.PagedLists
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField
import java.util.LinkedHashMap

private const val TAG = "bae.LibraryBrowseTree"
private const val ACCESS_ORDER_INITIAL_CAPACITY = 16
private const val ACCESS_ORDER_LOAD_FACTOR = 0.75f
private val logger = BaeLogger(TAG)

internal data class BrowseLabels(
    val library: String,
    val albums: String,
    val composers: String,
)

private class ParentInterest

private data class ParentInterests(
    val explicit: MutableMap<String, ParentInterest> = mutableMapOf(),
    val implicit: LinkedHashMap<String, ParentInterest> =
        LinkedHashMap(ACCESS_ORDER_INITIAL_CAPACITY, ACCESS_ORDER_LOAD_FACTOR, true),
) {
    val entries: List<Pair<String, ParentInterest>>
        get() = explicit.toList() + implicit.toList()

    val isEmpty: Boolean
        get() = explicit.isEmpty() && implicit.isEmpty()
}

private data class RetainedParent(
    val interest: ParentInterest,
    val evictedParentIds: List<String>,
)

private fun ParentInterests.retain(
    parentId: String,
    explicitly: Boolean,
    maximumImplicitCount: Int,
): RetainedParent =
    if (explicitly) {
        val retained = explicit[parentId] ?: implicit.remove(parentId) ?: ParentInterest()
        explicit[parentId] = retained
        RetainedParent(retained, emptyList())
    } else {
        val explicitlyRetained = explicit[parentId]
        if (explicitlyRetained != null) {
            RetainedParent(explicitlyRetained, emptyList())
        } else {
            val retained = implicit[parentId] ?: ParentInterest()
            implicit[parentId] = retained
            val evicted = mutableListOf<String>()
            while (implicit.size > maximumImplicitCount) {
                implicit.entries.first().key.also {
                    implicit.remove(it)
                    evicted += it
                }
            }
            RetainedParent(retained, evicted)
        }
    }

internal class LibraryBrowseTree<Owner : Any>(
    /**
     * The open library's handle. The tree makes one-time reads on it and opens its live list queries through
     * [lists].
     */
    private val handle: AppHandleInterface,
    private val labels: () -> BrowseLabels,
    private val nodes: BrowseNodeFactory,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined),
    private val onChildrenChanged: (parentId: String, itemCount: Int) -> Unit = { _, _ -> },
) {
    private val lists = PagedLists(handle)

    private val onQueryError: (BridgeException) -> Unit = { error ->
        logger.error("library browse live query failed", error)
    }

    val root: MediaItem
        get() = nodes.browsable(BrowseId.Root, labels().library, null, MediaMetadata.MEDIA_TYPE_FOLDER_MIXED)
    private val lock = Any()
    private val parentsByOwner = mutableMapOf<Owner, ParentInterests>()

    /** The live pages of each retained album's tracks and each retained work's children, by its media id. */
    private val childPages = mutableMapOf<String, ChildPages<*>>()

    /** Each controller's search: its phrase, with the live count and pages of the albums the phrase lists. */
    private val searchesByOwner = mutableMapOf<Owner, Search>()
    private var albums: ListProjection<BridgeAlbumAppearance, BridgeAlbum>? = null
    private var composers: ListProjection<BridgeComposerSummary, BridgeComposerSummary>? = null

    /** Each retained composer's page rows, by the composer's media id. */
    private val composerPages = mutableMapOf<String, ListProjection<BridgeComposerDetailRow, BridgeComposerDetailRow>>()
    private var closed = false

    /** [parentId]'s children on the page asked for, or null for a parent or page that names nothing. */
    suspend fun children(
        parentId: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem>? {
        checkOpen()
        if (!BrowsePaging.isPage(page, pageSize)) {
            logger.warning("a controller asked for page $page of size $pageSize of $parentId, which names no page")
            return null
        }
        return when (val id = BrowseId.parse(parentId)) {
            null -> {
                null
            }

            BrowseId.Root -> {
                val names = labels()
                BrowsePaging.categoryPage(
                    listOf(
                        nodes.browsable(BrowseId.Albums, names.albums, null, MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS),
                        nodes.browsable(
                            BrowseId.Composers,
                            names.composers,
                            null,
                            MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS,
                        ),
                    ),
                    page,
                    pageSize,
                )
            }

            BrowseId.Albums -> {
                pageAt(page, pageSize) { offset ->
                    albumProjection().rows(offset, pageSize).map { nodes.album(it.id, it.title, it.cover) }
                }
            }

            BrowseId.Composers -> {
                pageAt(page, pageSize) { offset -> composerProjection().rows(offset, pageSize).map(nodes::composer) }
            }

            is BrowseId.Composer -> {
                pageAt(page, pageSize) { offset ->
                    composerPageProjection(parentId, id.artistId).rows(offset, pageSize).map(nodes::composerPageRow)
                }
            }

            is BrowseId.Album, is BrowseId.Work -> {
                pageAt(page, pageSize) { offset -> childPages(parentId).rows(offset, pageSize) }
            }

            is BrowseId.Track -> {
                emptyList()
            }
        }
    }

    suspend fun item(mediaId: String): MediaItem? {
        checkOpen()
        return when (val id = BrowseId.parse(mediaId)) {
            null -> {
                null
            }

            BrowseId.Root -> {
                root
            }

            BrowseId.Albums -> {
                nodes.browsable(BrowseId.Albums, labels().albums, null, MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS)
            }

            BrowseId.Composers -> {
                nodes.browsable(BrowseId.Composers, labels().composers, null, MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS)
            }

            is BrowseId.Album -> {
                reportingFailure(onQueryError) { handle.readAlbumDetail(id.albumId) }?.let {
                    nodes.album(it.album.id, it.album.title, it.album.cover)
                }
            }

            is BrowseId.Composer -> {
                reportingFailure(onQueryError) { handle.readComposerDetail(id.artistId) }?.let {
                    nodes.composer(it.composer)
                }
            }

            is BrowseId.Work -> {
                reportingFailure(onQueryError) { handle.readWorkDetail(id.workId) }?.let { nodes.work(it.work) }
            }

            is BrowseId.Track -> {
                reportingFailure(onQueryError) { handle.readReleaseTrack(id.releaseId, id.trackId) }
                    ?.let(nodes::browseTrack)
            }
        }
    }

    /**
     * The albums [owner]'s search for [query] lists on the page asked for, read live from now on, or null for a page
     * that names nothing or a phrase [owner] is not searching for.
     */
    suspend fun search(
        owner: Owner,
        query: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem>? {
        checkOpen()
        val search = synchronized(lock) { searchesByOwner[owner] }?.takeIf { it.query == query }
        return when {
            !BrowsePaging.isPage(page, pageSize) -> {
                logger.warning("a controller asked for search page $page of size $pageSize, which names no page")
                null
            }

            search == null -> {
                logger.warning("a controller asked for the results of a phrase it is not searching for")
                null
            }

            else -> {
                pageAt(page, pageSize) { offset -> search.pages.rows(offset, pageSize) }
            }
        }
    }

    suspend fun subscribeParent(
        owner: Owner,
        parentId: String,
    ): Boolean = retainParent(owner, parentId, explicit = true)

    suspend fun retainImplicitParent(
        owner: Owner,
        parentId: String,
    ): Boolean = retainParent(owner, parentId, explicit = false)

    private suspend fun retainParent(
        owner: Owner,
        parentId: String,
        explicit: Boolean,
    ): Boolean {
        checkOpen()
        if (BrowseId.parse(parentId) == null) return false
        val retainedParent =
            synchronized(lock) {
                parentsByOwner.getOrPut(owner, ::ParentInterests).retain(
                    parentId,
                    explicit,
                    MAXIMUM_IMPLICIT_PARENT_INTERESTS,
                )
            }
        retainedParent.evictedParentIds.forEach { closeParentIfUnused(it) }
        when (val id = BrowseId.parse(parentId)) {
            BrowseId.Albums -> {
                albumProjection().awaitReady()
            }

            BrowseId.Composers -> {
                composerProjection().awaitReady()
            }

            is BrowseId.Composer -> {
                composerPageProjection(parentId, id.artistId).awaitReady()
            }

            is BrowseId.Album, is BrowseId.Work -> {
                childPages(parentId).awaitReady()
            }

            else -> {
                Unit
            }
        }
        val retained =
            synchronized(lock) {
                parentsByOwner[owner]?.let { interests ->
                    interests.explicit[parentId] === retainedParent.interest ||
                        interests.implicit[parentId] === retainedParent.interest
                } == true
            }
        if (!retained) throw parentInterestEnded()
        return true
    }

    fun unsubscribeParent(
        owner: Owner,
        parentId: String,
    ) {
        synchronized(lock) {
            parentsByOwner[owner]?.let { interests ->
                interests.explicit.remove(parentId)
                if (interests.isEmpty) parentsByOwner.remove(owner)
            }
        }
        runBlocking { closeParentIfUnused(parentId) }
    }

    /**
     * Search [query] for [owner], replacing its last search, and wait for the first count of the
     * albums it lists. That count and each later one reach [onResultsChanged], and so does each
     * change to a page of the albums that [search] read, with the number of albums unknown.
     */
    suspend fun subscribeSearch(
        owner: Owner,
        query: String,
        onResultsChanged: (Int) -> Unit,
    ) {
        checkOpen()
        val search = Search(owner, query, onResultsChanged)
        val (opened, replaced) =
            synchronized(lock) {
                if (closed) false to null else true to searchesByOwner.put(owner, search)
            }
        if (!opened) {
            search.pages.close(searchInterestEnded())
            throw treeClosedError()
        }
        replaced?.pages?.close(searchInterestEnded())
        search.pages.start()
        search.pages.awaitReady()
        if (!synchronized(lock) { searchesByOwner[owner] === search }) throw searchInterestEnded()
    }

    fun disconnect(owner: Owner) =
        runBlocking {
            val (parents, search) =
                synchronized(lock) {
                    parentsByOwner
                        .remove(owner)
                        ?.entries
                        .orEmpty()
                        .map { it.first } to searchesByOwner.remove(owner)
                }
            search?.pages?.close(searchInterestEnded())
            parents.forEach { closeParentIfUnused(it) }
        }

    suspend fun searchTopPlayable(query: String): BrowseId.Track? {
        checkOpen()
        val top = reportingFailure(onQueryError) { handle.readSearchTopPlayable(query) }
        return top?.let { BrowseId.Track(it.releaseId, it.trackId) }
    }

    fun close() = runBlocking { closeSuspending() }

    private suspend fun closeSuspending() {
        val pages = mutableListOf<ListProjection<BridgeComposerDetailRow, BridgeComposerDetailRow>>()
        val children = mutableListOf<ChildPages<*>>()
        val state =
            synchronized(lock) {
                if (closed) return
                closed = true
                val state = Pair(albums to composers, searchesByOwner.values.toList())
                pages += composerPages.values
                children += childPages.values
                albums = null
                composers = null
                composerPages.clear()
                childPages.clear()
                searchesByOwner.clear()
                parentsByOwner.clear()
                state
            }
        state.first.first?.close()
        state.first.second?.close()
        pages.forEach { it.close() }
        children.forEach { it.close() }
        state.second.forEach { it.pages.close(searchInterestEnded()) }
    }

    /**
     * [owner]'s search for [query]: the live count and pages of the albums the phrase lists. Each count, the first
     * too, and each change to a page reach [onResultsChanged] only while the tree holds this search as [owner]'s.
     */
    private inner class Search(
        owner: Owner,
        val query: String,
        onResultsChanged: (Int) -> Unit,
    ) {
        val pages =
            ChildPages(
                scope,
                lists.searchAlbumPages(query),
                { nodes.album(it.id, it.title, it.cover) },
                { count, _ -> if (synchronized(lock) { searchesByOwner[owner] === this }) onResultsChanged(count) },
                onQueryError,
            )
    }

    private fun albumProjection(): ListProjection<BridgeAlbumAppearance, BridgeAlbum> =
        synchronized(lock) {
            check(!closed)
            albums ?: ListProjection(
                scope,
                lists.albumPages(listOf(ALBUM_SORT)),
                BridgeAlbumAppearance::album,
                onChanged = { count -> notifyParent(BrowseId.Albums.mediaId, count) },
                onError = onQueryError,
            ).also { albums = it }
        }

    private fun composerProjection(): ListProjection<BridgeComposerSummary, BridgeComposerSummary> =
        synchronized(lock) {
            check(!closed)
            composers ?: ListProjection(
                scope,
                lists.composerPages(COMPOSER_SORT),
                { it },
                onChanged = { count -> notifyParent(BrowseId.Composers.mediaId, count) },
                onError = onQueryError,
            ).also { composers = it }
        }

    /** [artistId]'s page rows for the composer node [parentId], opened on first use. */
    private fun composerPageProjection(
        parentId: String,
        artistId: String,
    ): ListProjection<BridgeComposerDetailRow, BridgeComposerDetailRow> =
        synchronized(lock) {
            check(!closed)
            composerPages[parentId] ?: ListProjection(
                scope,
                lists.composerPagePages(artistId),
                { it },
                onChanged = { count -> notifyParent(parentId, count) },
                onError = onQueryError,
            ).also { composerPages[parentId] = it }
        }

    /** The live pages of [parentId]'s children, an album's tracks or a work's children, opened on first use. */
    private fun childPages(parentId: String): ChildPages<*> =
        synchronized(lock) {
            check(!closed)
            childPages.getOrPut(parentId) {
                // The tree tells the controller each change to a parent's count, and not the first count, which the
                // controller's subscription waits for.
                val onChanged = { count: Int, isFirst: Boolean -> if (!isFirst) notifyParent(parentId, count) }
                when (val id = BrowseId.parse(parentId)) {
                    is BrowseId.Album -> {
                        ChildPages(
                            scope,
                            lists.albumTrackPages(id.albumId),
                            nodes::browseTrack,
                            onChanged,
                            onQueryError,
                        )
                    }

                    is BrowseId.Work -> {
                        ChildPages(
                            scope,
                            lists.workChildPages(id.workId),
                            nodes::workChild,
                            onChanged,
                            onQueryError,
                        )
                    }

                    else -> {
                        error("$parentId lists no children a page at a time")
                    }
                }.also { it.start() }
            }
        }

    private suspend fun closeParentIfUnused(parentId: String) {
        val retained =
            synchronized(lock) {
                parentsByOwner.values.any { parentId in it.explicit || parentId in it.implicit }
            }
        if (retained) return
        when (BrowseId.parse(parentId)) {
            BrowseId.Albums -> synchronized(lock) { albums.also { albums = null } }?.close()
            BrowseId.Composers -> synchronized(lock) { composers.also { composers = null } }?.close()
            is BrowseId.Composer -> synchronized(lock) { composerPages.remove(parentId) }?.close()
            is BrowseId.Album, is BrowseId.Work -> synchronized(lock) { childPages.remove(parentId) }?.close()
            BrowseId.Root, is BrowseId.Track, null -> Unit
        }
    }

    private fun notifyParent(
        parentId: String,
        count: Int,
    ) {
        val interested =
            synchronized(lock) {
                !closed && parentsByOwner.values.any { parentId in it.explicit || parentId in it.implicit }
            }
        if (interested) {
            val stillInterested =
                synchronized(lock) {
                    !closed && parentsByOwner.values.any { parentId in it.explicit || parentId in it.implicit }
                }
            if (stillInterested) onChildrenChanged(parentId, count)
        }
    }

    private fun checkOpen() {
        synchronized(lock) {
            if (closed) throw treeClosedError()
        }
    }

    private companion object {
        val ALBUM_SORT = BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING)
        val COMPOSER_SORT = BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING)
        const val MAXIMUM_IMPLICIT_PARENT_INTERESTS = 12
    }
}

internal fun pageReplaced(): BrowseUnavailable = BrowseUnavailable.Superseded("browse page replaced")

internal fun treeClosedError(): BrowseUnavailable = BrowseUnavailable.LibraryClosed()

private fun parentInterestEnded(): BrowseUnavailable = BrowseUnavailable.Superseded("parent interest ended")

private fun searchInterestEnded(): BrowseUnavailable = BrowseUnavailable.Superseded("search interest ended")

/** The items [read] answers for the page's offset, none when the offset is past what an int holds. */
private suspend fun <Item> pageAt(
    page: Int,
    pageSize: Int,
    read: suspend (Int) -> List<Item>,
): List<Item> {
    val offset = page.toLong() * pageSize
    return if (offset > Int.MAX_VALUE) emptyList() else read(offset.toInt())
}
