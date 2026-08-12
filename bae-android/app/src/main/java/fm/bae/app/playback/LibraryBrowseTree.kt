package fm.bae.app.playback

import android.net.Uri
import androidx.annotation.VisibleForTesting
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import fm.bae.app.BaeLogger
import fm.bae.app.data.Library
import fm.bae.app.data.LiveQueryEvent
import fm.bae.app.data.mapValue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.selects.select
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField
import uniffi.bae_bridge.BridgeTrack
import java.util.LinkedHashMap

private const val TAG = "bae.LibraryBrowseTree"
private const val PARENT_INTEREST_MAP_INITIAL_CAPACITY = 16
private const val PARENT_INTEREST_MAP_LOAD_FACTOR = 0.75f
private val logger = BaeLogger(TAG)

/**
 * The two top-level category names the root exposes. Resolved by the caller
 * through the platform string catalog (the tree has no locale), so the tree
 * builds every node's user-visible text without touching resources itself.
 */
internal data class BrowseLabels(
    val albums: String,
    val composers: String,
)

private data class BrowsePage(
    val items: List<MediaItem>,
    val totalCount: Int,
)

private data class ParentPageKey(
    val parentId: String,
    val page: Int,
    val pageSize: Int,
)

private class ParentInterest {
    val ended = CompletableDeferred<Unit>()

    fun end() {
        ended.complete(Unit)
    }
}

private data class ParentInterests(
    val explicit: MutableMap<String, ParentInterest> = mutableMapOf(),
    val implicit: LinkedHashMap<String, ParentInterest> =
        LinkedHashMap(PARENT_INTEREST_MAP_INITIAL_CAPACITY, PARENT_INTEREST_MAP_LOAD_FACTOR, true),
) {
    val parentIds: Set<String>
        get() = explicit.keys + implicit.keys

    val entries: List<Pair<String, ParentInterest>>
        get() = explicit.toList() + implicit.toList()

    val isEmpty: Boolean
        get() = explicit.isEmpty() && implicit.isEmpty()
}

/**
 * Builds the media-browse tree Android Auto / Bluetooth head units navigate,
 * served from the same paged library reads the in-app browser uses ([Library]).
 * The shape mirrors the app's browser: root → Albums / Composers; an album
 * drills to its primary release's tracks; a composer drills to its works and
 * credited albums, and a work to its child works and releases.
 *
 * Queries are live and paged: the top-level album and composer lists honor the
 * browser's requested page window without loading the whole library. Active
 * controller interests retain their projections until unsubscribe or
 * disconnect; delivered inactive pages remain in bounded caches. Nodes
 * themselves are built by [BrowseNodeFactory].
 */
internal class LibraryBrowseTree<Owner : Any>(
    private val library: Library,
    /** The category labels, resolved through the platform string catalog on
     *  demand (not at construction) so a browse request picks up the current
     *  locale and the service does no resource work until a client browses. */
    private val labels: () -> BrowseLabels,
    /** Maps a cover-image id (a release/composer/work cover) to the content URI
     *  the browse client fetches its bytes from — the same bytes the bridge's
     *  `fetchLibraryImageBytes` serves. */
    artworkUri: (image: BridgeImageRef) -> Uri,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined),
    private val onChildrenChanged: (parentId: String, itemCount: Int) -> Unit = { _, _ -> },
    private val onQueryError: (BridgeException) -> Unit = { error ->
        logger.error("library browse live query failed", error)
    },
) {
    private data class SearchInterest(
        val query: String,
        val identity: Any,
        val onResultsChanged: (Int) -> Unit,
    )

    private val nodes = BrowseNodeFactory(artworkUri)
    private val interestLock = Any()
    private val parentsByOwner = mutableMapOf<Owner, ParentInterests>()
    private val searchesByOwner = mutableMapOf<Owner, SearchInterest>()
    private val pageProjections =
        LiveProjectionCache(
            scope = scope,
            maximumCount = MAXIMUM_PAGE_SUBSCRIPTIONS,
            flow = ::pageFlow,
            isRetained = { false },
            onError = { _, _, error, _ -> onQueryError(error) },
        )
    private val parentObservers =
        LiveProjectionCache(
            scope = scope,
            maximumCount = Int.MAX_VALUE,
            flow = ::parentCountFlow,
            isRetained = ::isParentRetained,
            onChanged = { parentId, count ->
                retirePagesAndNotify(parentId, count)
            },
            onError = { parentId, previous, error, isUpdate ->
                onQueryError(error)
                if (isUpdate) retirePagesAndNotify(parentId, previous ?: 0)
            },
        )
    private val albumDetails = exactProjections(library::albumDetails)
    private val composerDetails = exactProjections(library::composerDetails)
    private val workDetails = exactProjections(library::workDetails)
    private val releaseDetails = exactProjections(library::releaseDetails)
    private val searchObservers =
        LiveProjectionCache(
            scope = scope,
            maximumCount = MAXIMUM_SEARCH_SUBSCRIPTIONS,
            flow = library::searchResults,
            isRetained = ::isSearchRetained,
            onChanged = { query, results -> notifySearchResults(query, results.albums.size) },
            onError = { query, previous, error, isUpdate ->
                onQueryError(error)
                if (isUpdate) notifySearchResults(query, previous?.albums?.size ?: 0)
            },
        )
    private val spokenSearches = exactProjections(library::searchResults)

    /** The tree root. Its [children] are the top-level categories. */
    fun root(): MediaItem =
        nodes.browsable(
            id = BrowseId.Root,
            title = ROOT_TITLE,
            cover = null,
            mediaType = MediaMetadata.MEDIA_TYPE_FOLDER_MIXED,
        )

    /**
     * Children of [parentId] for the requested page window, or null when the id
     * names no node in the tree (the caller reports a bad-id error). An empty
     * list is a valid answer — a node that legitimately has no children.
     */
    suspend fun children(
        parentId: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem>? =
        when (val id = BrowseId.parse(parentId)) {
            null -> {
                null
            }

            BrowseId.Root -> {
                val labels = labels()
                paginate(
                    listOf(
                        nodes.browsable(
                            BrowseId.Albums,
                            labels.albums,
                            null,
                            MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS,
                        ),
                        nodes.browsable(
                            BrowseId.Composers,
                            labels.composers,
                            null,
                            MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS,
                        ),
                    ),
                    page,
                    pageSize,
                )
            }

            BrowseId.Albums,
            BrowseId.Composers,
            is BrowseId.Album,
            is BrowseId.Composer,
            is BrowseId.Work,
            -> pageProjections.value(ParentPageKey(parentId, page, pageSize)).items

            is BrowseId.Track -> {
                // A track is a leaf (playable, not browsable); a browse client
                // should never ask for its children.
                logger.debug("children requested for playable track ${id.releaseId}#${id.index}; none")
                emptyList()
            }
        }

    /** The single node [mediaId] names, or null when it names none. */
    suspend fun item(mediaId: String): MediaItem? =
        when (val id = BrowseId.parse(mediaId)) {
            null -> {
                null
            }

            BrowseId.Root -> {
                root()
            }

            BrowseId.Albums -> {
                nodes.browsable(BrowseId.Albums, labels().albums, null, MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS)
            }

            BrowseId.Composers -> {
                nodes.browsable(BrowseId.Composers, labels().composers, null, MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS)
            }

            is BrowseId.Album -> {
                val detail = albumDetails.value(id.albumId) ?: return null
                nodes.album(detail.album.id, detail.album.title, detail.album.cover)
            }

            is BrowseId.Composer -> {
                val detail = composerDetails.value(id.artistId) ?: return null
                nodes.composer(detail.composer)
            }

            is BrowseId.Work -> {
                val detail = workDetails.value(id.workId) ?: return null
                nodes.work(detail.work)
            }

            is BrowseId.Track -> {
                val release = releaseDetails.value(id.releaseId) ?: return null
                val track = flatTracks(release).getOrNull(id.index) ?: return null
                nodes.track(release, track, id.index)
            }
        }

    /**
     * Search results for the head unit's search screen, as browsable album
     * nodes (the client drills into one to play its tracks). Sliced to the
     * requested page window.
     */
    suspend fun search(
        query: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val albums = searchObservers.value(query).albums.map { nodes.album(it.id, it.title, it.cover) }
        return paginate(albums, page, pageSize)
    }

    suspend fun subscribeParent(
        owner: Owner,
        parentId: String,
    ): Boolean {
        if (BrowseId.parse(parentId) == null) return false
        val interest =
            synchronized(interestLock) {
                parentsByOwner.getOrPut(owner, ::ParentInterests).let { interests ->
                    interests.explicit[parentId]
                        ?: interests.implicit.remove(parentId)?.also { interests.explicit[parentId] = it }
                        ?: ParentInterest().also { interests.explicit[parentId] = it }
                }
            }
        if (isObservableParent(parentId)) {
            awaitParentObservation(parentId, interest, parentObservers)
        }
        return true
    }

    suspend fun retainImplicitParent(
        owner: Owner,
        parentId: String,
    ): Boolean {
        if (BrowseId.parse(parentId) == null) return false
        val (interest, evicted) =
            synchronized(interestLock) {
                val interests = parentsByOwner.getOrPut(owner, ::ParentInterests)
                val retained = interests.explicit[parentId]
                if (retained != null) {
                    retained to emptyList()
                } else {
                    val implicit = interests.implicit[parentId] ?: ParentInterest()
                    interests.implicit[parentId] = implicit
                    implicit to
                        buildList {
                            while (interests.implicit.size > MAXIMUM_IMPLICIT_PARENT_INTERESTS) {
                                val eldest = interests.implicit.entries.first()
                                interests.implicit.remove(eldest.key)
                                add(eldest.key to eldest.value)
                            }
                        }
                }
            }
        evicted.forEach { (evictedParent, evictedInterest) ->
            evictedInterest.end()
            if (!isParentRetained(evictedParent)) parentObservers.cancelWhenUnused(evictedParent)
        }
        if (isObservableParent(parentId)) {
            awaitParentObservation(parentId, interest, parentObservers)
        }
        return true
    }

    fun unsubscribeParent(
        owner: Owner,
        parentId: String,
    ) {
        val (ended, retained) =
            synchronized(interestLock) {
                val ended =
                    parentsByOwner[owner]?.let { interests ->
                        interests.explicit.remove(parentId).also {
                            if (interests.isEmpty) parentsByOwner.remove(owner)
                        }
                    }
                ended to parentsByOwner.values.any { parentId in it.parentIds }
            }
        ended?.end()
        if (!retained) {
            parentObservers.cancelWhenUnused(parentId)
        }
    }

    suspend fun subscribeSearch(
        owner: Owner,
        query: String,
        onResultsChanged: (Int) -> Unit,
    ) {
        val identity = Any()
        val previousQuery =
            synchronized(interestLock) {
                searchesByOwner.put(owner, SearchInterest(query, identity, onResultsChanged))?.query
            }
        searchObservers.ensure(query)
        if (previousQuery != null && previousQuery != query && !isSearchRetained(previousQuery)) {
            searchObservers.cancelWhenUnused(previousQuery)
        }
        val count = searchObservers.value(query).albums.size
        synchronized(interestLock) {
            searchesByOwner[owner]
                ?.takeIf { it.identity === identity }
                ?.onResultsChanged(count)
        }
    }

    fun disconnect(owner: Owner) {
        val (parents, query) =
            synchronized(interestLock) {
                parentsByOwner.remove(owner)?.entries.orEmpty() to searchesByOwner.remove(owner)?.query
            }
        parents.forEach { (parentId, interest) ->
            interest.end()
            if (!isParentRetained(parentId)) {
                parentObservers.cancelWhenUnused(parentId)
            }
        }
        query?.let { query ->
            if (!isSearchRetained(query)) searchObservers.cancelWhenUnused(query)
        }
    }

    /**
     * The track a spoken "play X" should start, or null when the search finds
     * nothing playable. Prefers a matching track (started in its primary
     * release); falls back to the top album's first track. A track result
     * short-circuits: if there is one but its album has no primary release, the
     * answer is null — the search does not then fall back to the album list.
     */
    suspend fun searchTopPlayable(query: String): BrowseId.Track? {
        val results = spokenSearches.value(query)
        val firstTrack = results.tracks.firstOrNull()
        val firstAlbum = results.albums.firstOrNull()
        return when {
            firstTrack != null -> {
                albumDetails.value(firstTrack.albumId)?.let(::primaryRelease)?.let { release ->
                    val index = flatTracks(release).indexOfFirst { it.id == firstTrack.id }
                    if (index < 0) {
                        logger.debug(
                            "spoken-match track ${firstTrack.id} not in primary release " +
                                "${release.id}; starting the release",
                        )
                    }
                    BrowseId.Track(release.id, index.coerceAtLeast(0))
                }
            }

            firstAlbum != null -> {
                albumDetails.value(firstAlbum.id)?.let(::primaryRelease)?.let { BrowseId.Track(it.id, 0) }
            }

            else -> {
                null
            }
        }
    }

    fun close() {
        val parentInterests =
            synchronized(interestLock) {
                parentsByOwner.values.flatMap(ParentInterests::entries).also {
                    parentsByOwner.clear()
                    searchesByOwner.clear()
                }
            }
        parentInterests.forEach { (_, interest) -> interest.end() }
        pageProjections.cancelAll()
        parentObservers.cancelAll()
        albumDetails.cancelAll()
        composerDetails.cancelAll()
        workDetails.cancelAll()
        releaseDetails.cancelAll()
        searchObservers.cancelAll()
        spokenSearches.cancelAll()
    }

    private fun isParentRetained(parentId: String): Boolean =
        synchronized(interestLock) { parentsByOwner.values.any { parentId in it.parentIds } }

    private fun isSearchRetained(query: String): Boolean =
        synchronized(interestLock) { searchesByOwner.values.any { it.query == query } }

    private fun retirePagesAndNotify(
        parentId: String,
        count: Int,
    ) {
        pageProjections.retireWhere(
            predicate = { key -> key.parentId == parentId },
            error =
                BridgeException.Diagnostic(
                    category = BridgeErrorCategory.DATABASE,
                    detail = "browse parent changed; retry the requested page",
                ),
        )
        onChildrenChanged(parentId, count)
    }

    private fun notifySearchResults(
        query: String,
        count: Int,
    ) {
        val listeners =
            synchronized(interestLock) {
                searchesByOwner
                    .filterValues { it.query == query }
                    .map { (owner, interest) -> owner to interest.identity }
            }
        listeners.forEach { (owner, identity) ->
            synchronized(interestLock) {
                searchesByOwner[owner]
                    ?.takeIf { it.identity === identity }
                    ?.onResultsChanged(count)
            }
        }
    }

    private fun <Key, Value> exactProjections(
        flow: (Key) -> Flow<LiveQueryEvent<Value>>,
    ): LiveProjectionCache<Key, Value> =
        LiveProjectionCache(
            scope,
            MAXIMUM_EXACT_SUBSCRIPTIONS,
            flow,
            isRetained = { false },
            onError = { _, _, error, _ -> onQueryError(error) },
        )

    private fun isObservableParent(parentId: String): Boolean =
        when (BrowseId.parse(parentId)) {
            BrowseId.Albums,
            BrowseId.Composers,
            is BrowseId.Album,
            is BrowseId.Composer,
            is BrowseId.Work,
            -> true

            null,
            BrowseId.Root,
            is BrowseId.Track,
            -> false
        }

    private fun parentCountFlow(parentId: String): Flow<LiveQueryEvent<Int>> =
        when (val id = checkNotNull(BrowseId.parse(parentId))) {
            BrowseId.Albums ->
                library.albumParentObservation().map { event ->
                    event.mapValue { it.childCount.toInt() }
                }

            BrowseId.Composers ->
                library.composerParentObservation().map { event ->
                    event.mapValue { it.childCount.toInt() }
                }

            is BrowseId.Album ->
                library.albumDetails(id.albumId).map { event ->
                    event.mapValue { detail -> detail?.let(::primaryRelease)?.let(::flatTracks)?.size ?: 0 }
                }

            is BrowseId.Composer ->
                library.composerDetails(id.artistId).map { event ->
                    event.mapValue { detail ->
                        detail?.let { value ->
                            value.workGroups.sumOf { group -> group.works.size + if (group.parent == null) 0 else 1 } +
                                value.unlinkedReleaseRoles.size
                        } ?: 0
                    }
                }

            is BrowseId.Work ->
                library.workDetails(id.workId).map { event ->
                    event.mapValue { detail -> detail?.let { it.childWorks.size + it.releases.size } ?: 0 }
                }

            BrowseId.Root,
            is BrowseId.Track,
            -> error("static browse id $parentId cannot create a parent observation")
        }

    private fun pageFlow(key: ParentPageKey): Flow<LiveQueryEvent<BrowsePage>> =
        when (val id = checkNotNull(BrowseId.parse(key.parentId))) {
            BrowseId.Albums ->
                library
                    .albumPages(listOf(ALBUM_SORT), offsetOf(key.page, key.pageSize), limitOf(key.pageSize))
                    .map { event ->
                        event.mapValue { page ->
                            BrowsePage(
                                page.rows.map { nodes.album(it.id, it.title, it.cover) },
                                page.totalCount.toInt(),
                            )
                        }
                    }

            BrowseId.Composers ->
                library
                    .composerPages(COMPOSER_SORT, offsetOf(key.page, key.pageSize), limitOf(key.pageSize))
                    .map { event ->
                        event.mapValue { page ->
                            BrowsePage(page.rows.map(nodes::composer), page.totalCount.toInt())
                        }
                    }

            is BrowseId.Album ->
                library.albumDetails(id.albumId).map { event ->
                    event.mapValue { detail ->
                        val items =
                            detail?.let(::primaryRelease)?.let { release ->
                                flatTracks(release).mapIndexed { index, track -> nodes.track(release, track, index) }
                            }.orEmpty()
                        BrowsePage(paginate(items, key.page, key.pageSize), items.size)
                    }
                }

            is BrowseId.Composer ->
                library.composerDetails(id.artistId).map { event ->
                    event.mapValue { detail ->
                        val items =
                            if (detail == null) {
                                logger.warning("composer ${id.artistId} has no detail; no browse children")
                                emptyList()
                            } else {
                                val works = detail.workGroups.flatMap { group -> listOfNotNull(group.parent) + group.works }
                                works.map(nodes::work) +
                                    detail.unlinkedReleaseRoles.map { nodes.album(it.albumId, it.albumTitle, null) }
                            }
                        BrowsePage(paginate(items, key.page, key.pageSize), items.size)
                    }
                }

            is BrowseId.Work ->
                library.workDetails(id.workId).map { event ->
                    event.mapValue { detail ->
                        val items =
                            if (detail == null) {
                                logger.warning("work ${id.workId} has no detail; no browse children")
                                emptyList()
                            } else {
                                detail.childWorks.map(nodes::work) +
                                    detail.releases.map { nodes.album(it.albumId, it.albumTitle, it.cover) }
                            }
                        BrowsePage(paginate(items, key.page, key.pageSize), items.size)
                    }
                }

            BrowseId.Root,
            is BrowseId.Track,
            -> error("static browse id ${key.parentId} cannot create a live page")
        }

    private companion object {
        const val ROOT_TITLE = "bae"

        // The browser's default album and composer orderings — newest albums
        // first, composers by name — matching the in-app browser's defaults.
        val ALBUM_SORT = BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING)
        val COMPOSER_SORT = BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING)

        // Upper bound on how many rows one page read pulls. A browse client that
        // subscribes without paging asks for Int.MAX_VALUE; cap it so a browse
        // read never pulls the whole library.
        const val MAX_PAGE_SIZE = 500
        const val MAXIMUM_PAGE_SUBSCRIPTIONS = 12
        const val MAXIMUM_EXACT_SUBSCRIPTIONS = 24
        const val MAXIMUM_SEARCH_SUBSCRIPTIONS = 8
        const val MAXIMUM_IMPLICIT_PARENT_INTERESTS = 12

        fun limitOf(pageSize: Int): ULong = pageSize.coerceIn(1, MAX_PAGE_SIZE).toULong()

        fun offsetOf(
            page: Int,
            pageSize: Int,
        ): ULong = (page.toLong().coerceAtLeast(0) * pageSize.toLong().coerceAtLeast(0)).toULong()

        @VisibleForTesting
        fun paginate(
            items: List<MediaItem>,
            page: Int,
            pageSize: Int,
        ): List<MediaItem> {
            val limit = pageSize.coerceIn(1, MAX_PAGE_SIZE)
            val start = (page.toLong().coerceAtLeast(0) * pageSize.toLong().coerceAtLeast(0))
            if (start >= items.size) return emptyList()
            return items.drop(start.toInt()).take(limit)
        }
    }
}

/** The album's primary release, or its first release when the primary id names
 *  none — the release a browse drill or spoken match starts from. */
private fun primaryRelease(detail: BridgeAlbumDetail): BridgeRelease? =
    detail.releases.firstOrNull { it.id == detail.album.primaryReleaseId }
        ?: detail.releases.firstOrNull()

/** The release-wide flat track order (across track groups) — the same
 *  flattening the album-detail screen taps into, so a track's position here is
 *  the start index `play_release` expects. */
private fun flatTracks(release: BridgeRelease): List<BridgeTrack> = release.trackGroups.flatMap { it.tracks }

private suspend fun <Value> awaitParentObservation(
    parentId: String,
    interest: ParentInterest,
    parentObservers: LiveProjectionCache<String, Value>,
) {
    coroutineScope {
        val observation =
            async(start = CoroutineStart.UNDISPATCHED) {
                parentObservers.event(parentId)
            }
        select {
            observation.onAwait {}
            interest.ended.onAwait {
                observation.cancel()
                throw parentInterestEnded()
            }
        }
    }
    if (interest.ended.isCompleted) throw parentInterestEnded()
}

private fun parentInterestEnded(): BridgeException =
    BridgeException.Diagnostic(
        category = BridgeErrorCategory.INTERNAL,
        detail = "browse parent interest ended before its observation became ready",
    )
