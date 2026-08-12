package fm.bae.app.playback

import android.net.Uri
import androidx.annotation.VisibleForTesting
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import fm.bae.app.BaeLogger
import fm.bae.app.data.Library
import fm.bae.app.data.LiveQueryEvent
import fm.bae.app.data.mapValue
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField
import uniffi.bae_bridge.BridgeTrack
import java.util.LinkedHashMap
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach

private const val TAG = "bae.LibraryBrowseTree"
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

private data class SearchPageKey(
    val query: String,
    val page: Int,
    val pageSize: Int,
)

private class LiveProjection<T>(
    private val scope: CoroutineScope,
    private val flow: Flow<LiveQueryEvent<T>>,
    private val onEvent: (LiveQueryEvent<T>, isUpdate: Boolean) -> Unit,
) {
    private data class Delivered<T>(val value: T)

    private val lock = Any()
    private var delivered: Delivered<T>? = null
    private val first = CompletableDeferred<LiveQueryEvent<T>>()
    private var eventDelivered = false
    private var started = false
    private var cancelled = false
    private var job: Job? = null

    fun start() {
        val shouldStart =
            synchronized(lock) {
                if (started || cancelled) {
                    false
                } else {
                    started = true
                    true
                }
            }
        if (!shouldStart) return
        val launched =
            flow
                .onEach { event ->
                    val isUpdate =
                        synchronized(lock) {
                            val update = eventDelivered
                            eventDelivered = true
                            if (event is LiveQueryEvent.Value) {
                                delivered = Delivered(event.value)
                            }
                            if (!first.isCompleted) first.complete(event)
                            update
                        }
                    onEvent(event, isUpdate)
                }.launchIn(scope)
        synchronized(lock) {
            if (cancelled) launched.cancel() else job = launched
        }
    }

    suspend fun value(): T {
        synchronized(lock) { delivered }?.let { return it.value }
        return when (val event = first.await()) {
            is LiveQueryEvent.Value -> event.value
            is LiveQueryEvent.Error -> throw event.error
        }
    }

    fun cancel() {
        val activeJob =
            synchronized(lock) {
                cancelled = true
                first.cancel()
                job
            }
        activeJob?.cancel()
    }
}

private class LiveProjectionCache<Key, Value>(
    private val scope: CoroutineScope,
    private val maximumCount: Int,
    private val flow: (Key) -> Flow<LiveQueryEvent<Value>>,
    private val isRetained: (Key) -> Boolean,
    private val onChanged: (Key, Value) -> Unit = { _, _ -> },
    private val onError: (Key, BridgeException) -> Unit,
) {
    private class Entry<Value>(
        val identity: Any,
        val projection: LiveProjection<Value>,
        var waiters: Int = 0,
        var cancelWhenUnused: Boolean = false,
    )

    private val lock = Any()
    private val projections = LinkedHashMap<Key, Entry<Value>>(16, 0.75f, true)

    suspend fun value(key: Key): Value {
        val entry = acquire(key)
        return try {
            entry.projection.value()
        } finally {
            release(key, entry.identity)
        }
    }

    fun ensure(key: Key) {
        var created = false
        val entry =
            synchronized(lock) {
                (projections[key]
                    ?: createEntry(key).also {
                        projections[key] = it
                        created = true
                    }).also { it.cancelWhenUnused = false }
            }
        trimAndCancel()
        if (created) entry.projection.start()
    }

    fun cancelWhenUnused(key: Key) {
        val cancelled =
            synchronized(lock) {
                projections[key]?.let { entry ->
                    if (isRetained(key)) {
                        entry.cancelWhenUnused = false
                        null
                    } else if (entry.waiters == 0) {
                        projections.remove(key)
                    } else {
                        entry.cancelWhenUnused = true
                        null
                    }
                }
            }
        cancelled?.projection?.cancel()
    }

    fun cancelAll() {
        val cancelled =
            synchronized(lock) {
                projections.values.toList().also { projections.clear() }
            }
        cancelled.forEach { it.projection.cancel() }
    }

    private fun acquire(key: Key): Entry<Value> {
        var created = false
        val entry =
            synchronized(lock) {
                (projections[key]
                    ?: createEntry(key).also {
                        projections[key] = it
                        created = true
                    }).also { it.waiters++ }
            }
        trimAndCancel()
        if (created) entry.projection.start()
        return entry
    }

    private fun createEntry(key: Key): Entry<Value> {
        val identity = Any()
        val projection =
            LiveProjection(scope, flow(key)) { event, isUpdate ->
                apply(key, identity, event, isUpdate)
            }
        return Entry(identity, projection)
    }

    private fun release(
        key: Key,
        identity: Any,
    ) {
        val cancelled =
            synchronized(lock) {
                projections[key]?.takeIf { it.identity === identity }?.let { entry ->
                    check(entry.waiters > 0)
                    entry.waiters--
                    if (entry.waiters == 0 && entry.cancelWhenUnused && !isRetained(key)) {
                        projections.remove(key)
                    } else {
                        null
                    }
                }
            }
        cancelled?.projection?.cancel()
        trimAndCancel()
    }

    private fun apply(
        key: Key,
        identity: Any,
        event: LiveQueryEvent<Value>,
        isUpdate: Boolean,
    ) {
        synchronized(lock) {
            if (projections[key]?.identity !== identity) return
            when (event) {
                is LiveQueryEvent.Value -> if (isUpdate) onChanged(key, event.value)
                is LiveQueryEvent.Error -> onError(key, event.error)
            }
        }
    }

    private fun trimAndCancel() {
        val evicted =
            synchronized(lock) {
                buildList {
                    while (projections.size > maximumCount) {
                        val candidate =
                            projections.entries.firstOrNull { (key, entry) ->
                                entry.waiters == 0 && !isRetained(key)
                            } ?: break
                        projections.remove(candidate.key)?.let(::add)
                    }
                }
            }
        evicted.forEach { it.projection.cancel() }
    }
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
    private data class SearchInterest(val onResultsChanged: (Int) -> Unit)

    private val nodes = BrowseNodeFactory(artworkUri)
    private val interestLock = Any()
    private val parentsByOwner = mutableMapOf<Owner, MutableSet<String>>()
    private val searchesByOwner = mutableMapOf<Owner, MutableMap<String, SearchInterest>>()
    private val pageProjections =
        LiveProjectionCache(
            scope = scope,
            maximumCount = MAXIMUM_PAGE_SUBSCRIPTIONS,
            flow = ::pageFlow,
            isRetained = { false },
            onError = { _, error -> onQueryError(error) },
        )
    private val parentObservers =
        LiveProjectionCache(
            scope = scope,
            maximumCount = Int.MAX_VALUE,
            flow = ::pageFlow,
            isRetained = { key -> isParentRetained(key.parentId) },
            onChanged = { key, value -> onChildrenChanged(key.parentId, value.totalCount) },
            onError = { _, error -> onQueryError(error) },
        )
    private val albumDetails = exactProjections(library::albumDetails)
    private val composerDetails = exactProjections(library::composerDetails)
    private val workDetails = exactProjections(library::workDetails)
    private val releaseDetails = exactProjections(library::releaseDetails)
    private val searchPages =
        LiveProjectionCache(
            scope = scope,
            maximumCount = MAXIMUM_SEARCH_SUBSCRIPTIONS,
            flow = ::searchPageFlow,
            isRetained = { false },
            onError = { _, error -> onQueryError(error) },
        )
    private val searchObservers =
        LiveProjectionCache(
            scope = scope,
            maximumCount = Int.MAX_VALUE,
            flow = library::searchResults,
            isRetained = ::isSearchRetained,
            onChanged = { query, value -> notifySearchResults(query, value.albums.size) },
            onError = { _, error -> onQueryError(error) },
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
        return searchPages.value(SearchPageKey(query, page, pageSize)).items
    }

    fun subscribeParent(
        owner: Owner,
        parentId: String,
    ): Boolean {
        if (BrowseId.parse(parentId) == null) return false
        synchronized(interestLock) {
            parentsByOwner.getOrPut(owner, ::mutableSetOf).add(parentId)
        }
        parentObservationKey(parentId)?.let(parentObservers::ensure)
        return true
    }

    fun unsubscribeParent(
        owner: Owner,
        parentId: String,
    ) {
        val retained =
            synchronized(interestLock) {
                parentsByOwner[owner]?.let { parents ->
                    parents.remove(parentId)
                    if (parents.isEmpty()) parentsByOwner.remove(owner)
                }
                parentsByOwner.values.any { parentId in it }
            }
        if (!retained) {
            parentObservationKey(parentId)?.let(parentObservers::cancelWhenUnused)
        }
    }

    suspend fun subscribeSearch(
        owner: Owner,
        query: String,
        onResultsChanged: (Int) -> Unit,
    ) {
        synchronized(interestLock) {
            searchesByOwner
                .getOrPut(owner, ::mutableMapOf)[query] = SearchInterest(onResultsChanged)
        }
        searchObservers.ensure(query)
        val count = searchObservers.value(query).albums.size
        if (isSearchOwned(owner, query)) onResultsChanged(count)
    }

    fun disconnect(owner: Owner) {
        val (parents, queries) =
            synchronized(interestLock) {
                parentsByOwner.remove(owner).orEmpty() to searchesByOwner.remove(owner)?.keys.orEmpty()
            }
        parents.forEach { parentId ->
            if (!isParentRetained(parentId)) {
                parentObservationKey(parentId)?.let(parentObservers::cancelWhenUnused)
            }
        }
        queries.forEach { query ->
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
        synchronized(interestLock) {
            parentsByOwner.clear()
            searchesByOwner.clear()
        }
        pageProjections.cancelAll()
        parentObservers.cancelAll()
        albumDetails.cancelAll()
        composerDetails.cancelAll()
        workDetails.cancelAll()
        releaseDetails.cancelAll()
        searchPages.cancelAll()
        searchObservers.cancelAll()
        spokenSearches.cancelAll()
    }

    private fun isParentRetained(parentId: String): Boolean =
        synchronized(interestLock) { parentsByOwner.values.any { parentId in it } }

    private fun isSearchRetained(query: String): Boolean =
        synchronized(interestLock) { searchesByOwner.values.any { query in it } }

    private fun isSearchOwned(
        owner: Owner,
        query: String,
    ): Boolean = synchronized(interestLock) { searchesByOwner[owner]?.containsKey(query) == true }

    private fun notifySearchResults(
        query: String,
        count: Int,
    ) {
        val listeners =
            synchronized(interestLock) {
                searchesByOwner.values.mapNotNull { it[query] }
            }
        listeners.forEach { it.onResultsChanged(count) }
    }

    private fun <Key, Value> exactProjections(
        flow: (Key) -> Flow<LiveQueryEvent<Value>>,
    ): LiveProjectionCache<Key, Value> =
        LiveProjectionCache(
            scope,
            MAXIMUM_EXACT_SUBSCRIPTIONS,
            flow,
            isRetained = { false },
            onError = { _, error -> onQueryError(error) },
        )

    private fun parentObservationKey(parentId: String): ParentPageKey? =
        when (BrowseId.parse(parentId)) {
            BrowseId.Albums,
            BrowseId.Composers,
            is BrowseId.Album,
            is BrowseId.Composer,
            is BrowseId.Work,
            -> ParentPageKey(parentId, page = 0, pageSize = 1)

            null,
            BrowseId.Root,
            is BrowseId.Track,
            -> null
        }

    private fun searchPageFlow(key: SearchPageKey): Flow<LiveQueryEvent<BrowsePage>> =
        library.searchResults(key.query).map { event ->
            event.mapValue { results ->
                val albums = results.albums.map { nodes.album(it.id, it.title, it.cover) }
                BrowsePage(paginate(albums, key.page, key.pageSize), albums.size)
            }
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
