package fm.bae.app.playback

import android.net.Uri
import androidx.annotation.VisibleForTesting
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import fm.bae.app.BaeLogger
import fm.bae.app.data.Library
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
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

private class LiveProjection<T>(
    scope: CoroutineScope,
    flow: Flow<T>,
    private val onChanged: (T) -> Unit,
) {
    private data class Delivered<T>(val value: T)

    private val lock = Any()
    private var delivered: Delivered<T>? = null
    private val first = CompletableDeferred<T>()
    private val job: Job =
        flow
            .onEach { value ->
                val isUpdate =
                    synchronized(lock) {
                        val prior = delivered
                        delivered = Delivered(value)
                        if (!first.isCompleted) first.complete(value)
                        prior != null
                    }
                if (isUpdate) onChanged(value)
            }.launchIn(scope)

    suspend fun value(): T {
        synchronized(lock) { delivered }?.let { return it.value }
        return first.await()
    }

    fun cancel() {
        first.cancel()
        job.cancel()
    }
}

private class LiveProjectionCache<Key, Value>(
    private val scope: CoroutineScope,
    private val maximumCount: Int,
    private val flow: (Key) -> Flow<Value>,
    private val onChanged: (Key, Value) -> Unit = { _, _ -> },
) {
    private val lock = Any()
    private val projections = LinkedHashMap<Key, LiveProjection<Value>>(16, 0.75f, true)

    suspend fun value(key: Key): Value = projection(key).value()

    fun cancel(key: Key) {
        synchronized(lock) { projections.remove(key) }?.cancel()
    }

    fun cancelWhere(predicate: (Key) -> Boolean) {
        val cancelled =
            synchronized(lock) {
                val keys = projections.keys.filter(predicate)
                keys.mapNotNull(projections::remove)
            }
        cancelled.forEach(LiveProjection<Value>::cancel)
    }

    fun cancelAll() {
        val cancelled =
            synchronized(lock) {
                projections.values.toList().also { projections.clear() }
            }
        cancelled.forEach(LiveProjection<Value>::cancel)
    }

    private fun projection(key: Key): LiveProjection<Value> {
        val evicted = mutableListOf<LiveProjection<Value>>()
        val projection =
            synchronized(lock) {
                projections[key]
                    ?: LiveProjection(scope, flow(key)) { onChanged(key, it) }.also {
                        projections[key] = it
                        while (projections.size > maximumCount) {
                            val eldest = projections.entries.iterator().next()
                            projections.remove(eldest.key)?.let(evicted::add)
                        }
                    }
            }
        evicted.forEach(LiveProjection<Value>::cancel)
        return projection
    }
}

/**
 * Builds the media-browse tree Android Auto / Bluetooth head units navigate,
 * served from the same paged library reads the in-app browser uses ([Library]).
 * The shape mirrors the app's browser: root → Albums / Composers; an album
 * drills to its primary release's tracks; a composer drills to its works and
 * credited albums, and a work to its child works and releases.
 *
 * Reads are `suspend` and paged: the top-level album and composer lists page
 * straight from the database honoring the browser's requested page window,
 * never loading the whole library. Detail-derived child lists (an album's
 * tracks, a composer's works) come from a single bounded detail read and are
 * sliced to the requested window. Nodes themselves are built by
 * [BrowseNodeFactory]; this type only reads and navigates.
 */
internal class LibraryBrowseTree(
    private val library: Library,
    /** The category labels, resolved through the platform string catalog on
     *  demand (not at construction) so a browse request picks up the current
     *  locale and the service does no resource work until a client browses. */
    private val labels: () -> BrowseLabels,
    /** Maps a cover-image id (a release/composer/work cover) to the content URI
     *  the browse client fetches its bytes from — the same bytes the bridge's
     *  `fetchLibraryImageBytes` serves. */
    artworkUri: (image: BridgeImageRef) -> Uri,
    scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined),
    private val onChildrenChanged: (parentId: String, itemCount: Int) -> Unit = { _, _ -> },
    private val onSearchResultChanged: (query: String, itemCount: Int) -> Unit = { _, _ -> },
) {
    private val nodes = BrowseNodeFactory(artworkUri)
    private val pageProjections =
        LiveProjectionCache(
            scope = scope,
            maximumCount = MAXIMUM_PAGE_SUBSCRIPTIONS,
            flow = ::pageFlow,
            onChanged = { key, value -> onChildrenChanged(key.parentId, value.totalCount) },
        )
    private val albumDetails =
        LiveProjectionCache(scope, MAXIMUM_EXACT_SUBSCRIPTIONS, library::albumDetails)
    private val composerDetails =
        LiveProjectionCache(scope, MAXIMUM_EXACT_SUBSCRIPTIONS, library::composerDetails)
    private val workDetails =
        LiveProjectionCache(scope, MAXIMUM_EXACT_SUBSCRIPTIONS, library::workDetails)
    private val releaseDetails =
        LiveProjectionCache(scope, MAXIMUM_EXACT_SUBSCRIPTIONS, library::releaseDetails)
    private val searches =
        LiveProjectionCache(
            scope = scope,
            maximumCount = MAXIMUM_SEARCH_SUBSCRIPTIONS,
            flow = library::searchResults,
            onChanged = { query, value -> onSearchResultChanged(query, value.albums.size) },
        )

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
        val albums = searches.value(query).albums.map { nodes.album(it.id, it.title, it.cover) }
        return paginate(albums, page, pageSize)
    }

    suspend fun searchCount(query: String): Int = searches.value(query).albums.size

    /**
     * The track a spoken "play X" should start, or null when the search finds
     * nothing playable. Prefers a matching track (started in its primary
     * release); falls back to the top album's first track. A track result
     * short-circuits: if there is one but its album has no primary release, the
     * answer is null — the search does not then fall back to the album list.
     */
    suspend fun searchTopPlayable(query: String): BrowseId.Track? {
        val results = searches.value(query)
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

    fun cancelParent(parentId: String) =
        pageProjections.cancelWhere { it.parentId == parentId }

    fun cancelSearch(query: String) = searches.cancel(query)

    fun close() {
        pageProjections.cancelAll()
        albumDetails.cancelAll()
        composerDetails.cancelAll()
        workDetails.cancelAll()
        releaseDetails.cancelAll()
        searches.cancelAll()
    }

    private fun pageFlow(key: ParentPageKey): Flow<BrowsePage> =
        when (val id = checkNotNull(BrowseId.parse(key.parentId))) {
            BrowseId.Albums ->
                library
                    .albumPages(listOf(ALBUM_SORT), offsetOf(key.page, key.pageSize), limitOf(key.pageSize))
                    .map { page ->
                        BrowsePage(
                            page.rows.map { nodes.album(it.id, it.title, it.cover) },
                            page.totalCount.toInt(),
                        )
                    }

            BrowseId.Composers ->
                library
                    .composerPages(COMPOSER_SORT, offsetOf(key.page, key.pageSize), limitOf(key.pageSize))
                    .map { page -> BrowsePage(page.rows.map(nodes::composer), page.totalCount.toInt()) }

            is BrowseId.Album ->
                library.albumDetails(id.albumId).map { detail ->
                    val items =
                        detail?.let(::primaryRelease)?.let { release ->
                            flatTracks(release).mapIndexed { index, track -> nodes.track(release, track, index) }
                        }.orEmpty()
                    BrowsePage(paginate(items, key.page, key.pageSize), items.size)
                }

            is BrowseId.Composer ->
                library.composerDetails(id.artistId).map { detail ->
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

            is BrowseId.Work ->
                library.workDetails(id.workId).map { detail ->
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
