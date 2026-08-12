package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import fm.bae.app.BaeLogger
import fm.bae.app.data.Library
import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.runBlocking
import uniffi.bae_bridge.BridgeAlbumBrowseSnapshot
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeComposerBrowseSnapshot
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeException.Diagnostic
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField
import uniffi.bae_bridge.BridgeTrack
import java.util.LinkedHashMap

private const val TAG = "bae.LibraryBrowseTree"
private const val ACCESS_ORDER_INITIAL_CAPACITY = 16
private const val ACCESS_ORDER_LOAD_FACTOR = 0.75f
private const val TREE_CLOSED_MESSAGE = "library browse tree is closed"
private val logger = BaeLogger(TAG)

internal data class BrowseLabels(
    val albums: String,
    val composers: String,
)

private data class BrowsePage(
    val items: List<MediaItem>,
    val totalCount: Int,
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
    private val library: Library,
    private val labels: () -> BrowseLabels,
    artworkUri: (image: BridgeImageRef) -> Uri,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined),
    private val onChildrenChanged: (parentId: String, itemCount: Int) -> Unit = { _, _ -> },
    private val onQueryError: (BridgeException) -> Unit = { error ->
        logger.error("library browse live query failed", error)
    },
) {
    private class SearchInterest(
        val identity: Any,
        val query: String,
        val listener: (Int) -> Unit,
        val projection: FixedProjection<BridgeSearchResults>,
    ) {
        var lastCount: Int? = null
    }

    private val nodes = BrowseNodeFactory(artworkUri)
    val root: MediaItem = nodes.browsable(BrowseId.Root, ROOT_TITLE, null, MediaMetadata.MEDIA_TYPE_FOLDER_MIXED)
    private val lock = Any()
    private val parentsByOwner = mutableMapOf<Owner, ParentInterests>()
    private val fixedParents = mutableMapOf<String, FixedProjection<BrowsePage>>()
    private val searchesByOwner = mutableMapOf<Owner, SearchInterest>()
    private val albumDetails = exactProjectionCache(scope, library::albumDetails, onQueryError)
    private val composerDetails = exactProjectionCache(scope, library::composerDetails, onQueryError)
    private val workDetails = exactProjectionCache(scope, library::workDetails, onQueryError)
    private val releaseDetails = exactProjectionCache(scope, library::releaseDetails, onQueryError)
    private val spokenSearches = exactProjectionCache(scope, library::searchResults, onQueryError)
    private var albums: CollectionProjection<uniffi.bae_bridge.BridgeAlbum, BridgeAlbumBrowseSnapshot>? = null
    private var composers:
        CollectionProjection<uniffi.bae_bridge.BridgeComposerSummary, BridgeComposerBrowseSnapshot>? = null
    private var closed = false

    suspend fun children(
        parentId: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem>? {
        checkOpen()
        return when (val id = BrowseId.parse(parentId)) {
            null -> {
                null
            }

            BrowseId.Root -> {
                val names = labels()
                BrowsePaging.paginate(
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
                albumProjection()
                    .rows(BrowsePaging.window(page, pageSize))
                    .map { nodes.album(it.id, it.title, it.cover) }
            }

            BrowseId.Composers -> {
                composerProjection().rows(BrowsePaging.window(page, pageSize)).map(nodes::composer)
            }

            is BrowseId.Album,
            is BrowseId.Composer,
            is BrowseId.Work,
            -> {
                BrowsePaging.paginate(fixedParent(parentId).value().items, page, pageSize)
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
                albumDetails.value(id.albumId)?.let { nodes.album(it.album.id, it.album.title, it.album.cover) }
            }

            is BrowseId.Composer -> {
                composerDetails.value(id.artistId)?.let { nodes.composer(it.composer) }
            }

            is BrowseId.Work -> {
                workDetails.value(id.workId)?.let { nodes.work(it.work) }
            }

            is BrowseId.Track -> {
                releaseDetails.value(id.releaseId)?.let { release ->
                    flatTracks(release).getOrNull(id.index)?.let { nodes.track(release, it, id.index) }
                }
            }
        }
    }

    suspend fun search(
        query: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        checkOpen()
        val projection = synchronized(lock) { searchesByOwner.values.firstOrNull { it.query == query }?.projection }
        val results = projection?.value() ?: spokenSearches.value(query)
        return BrowsePaging.paginate(results.albums.map { nodes.album(it.id, it.title, it.cover) }, page, pageSize)
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
        when (BrowseId.parse(parentId)) {
            BrowseId.Albums -> albumProjection().awaitReady()

            BrowseId.Composers -> composerProjection().awaitReady()

            is BrowseId.Album,
            is BrowseId.Composer,
            is BrowseId.Work,
            -> fixedParent(parentId).value()

            else -> Unit
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

    suspend fun subscribeSearch(
        owner: Owner,
        query: String,
        onResultsChanged: (Int) -> Unit,
    ) {
        checkOpen()
        val identity = Any()
        val projection =
            FixedProjection(
                scope,
                library.searchResults(query),
                onChanged = { value ->
                    val listener =
                        synchronized(lock) {
                            searchesByOwner[owner]
                                ?.takeIf { it.identity === identity }
                                ?.also {
                                    it.lastCount = value.albums.size
                                }?.listener
                        }
                    listener?.invoke(value.albums.size)
                },
                onError = { error ->
                    onQueryError(error)
                    val notification =
                        synchronized(lock) {
                            searchesByOwner[owner]?.takeIf { it.identity === identity }?.let { interest ->
                                interest.lastCount?.let { count -> interest.listener to count }
                            }
                        }
                    notification?.let { (listener, count) -> listener(count) }
                },
                notifyInitialValue = true,
                startImmediately = false,
            )
        val interest = SearchInterest(identity, query, onResultsChanged, projection)
        val previous =
            synchronized(lock) {
                if (closed) throw treeClosedError()
                searchesByOwner.put(owner, interest)
            }
        previous?.projection?.close()
        val retained = synchronized(lock) { searchesByOwner[owner] === interest }
        if (!retained) {
            projection.close()
            throw searchInterestEnded()
        }
        projection.start()
        projection.value()
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
            search?.projection?.close()
            parents.forEach { closeParentIfUnused(it) }
        }

    suspend fun searchTopPlayable(query: String): BrowseId.Track? {
        checkOpen()
        val results = spokenSearches.value(query)
        val firstTrack = results.tracks.firstOrNull()
        val firstAlbum = results.albums.firstOrNull()
        return when {
            firstTrack != null -> {
                albumDetails.value(firstTrack.albumId)?.let(::primaryRelease)?.let { release ->
                    val index = flatTracks(release).indexOfFirst { it.id == firstTrack.id }.coerceAtLeast(0)
                    BrowseId.Track(release.id, index)
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

    fun close() = runBlocking { closeSuspending() }

    private suspend fun closeSuspending() {
        val state =
            synchronized(lock) {
                if (closed) return
                closed = true
                val state = Triple(albums to composers, fixedParents.values.toList(), searchesByOwner.values.toList())
                albums = null
                composers = null
                fixedParents.clear()
                searchesByOwner.clear()
                parentsByOwner.clear()
                state
            }
        state.first.first?.close()
        state.first.second?.close()
        state.second.forEach { it.close() }
        state.third.forEach { it.projection.close() }
        val error = treeClosedError()
        albumDetails.cancelAll(error)
        composerDetails.cancelAll(error)
        workDetails.cancelAll(error)
        releaseDetails.cancelAll(error)
        spokenSearches.cancelAll(error)
    }

    private fun albumProjection(): CollectionProjection<uniffi.bae_bridge.BridgeAlbum, BridgeAlbumBrowseSnapshot> =
        synchronized(lock) {
            check(!closed)
            albums ?: albumCollectionProjection(
                scope,
                library.albumBrowse(listOf(ALBUM_SORT)),
                onChanged = { count -> notifyParent(BrowseId.Albums.mediaId, count) },
                onError = onQueryError,
            ).also { albums = it }
        }

    private fun composerProjection(): ComposerCollectionProjection =
        synchronized(lock) {
            check(!closed)
            composers ?: composerCollectionProjection(
                scope,
                library.composerBrowse(COMPOSER_SORT),
                onChanged = { count -> notifyParent(BrowseId.Composers.mediaId, count) },
                onError = onQueryError,
            ).also { composers = it }
        }

    private fun fixedParent(parentId: String): FixedProjection<BrowsePage> =
        synchronized(lock) {
            fixedParents[parentId] ?: FixedProjection(
                scope,
                parentFlow(parentId),
                onChanged = { notifyParent(parentId, it.totalCount) },
                onError = onQueryError,
            ).also { fixedParents[parentId] = it }
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
            else -> synchronized(lock) { fixedParents.remove(parentId) }?.close()
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

    private fun parentFlow(parentId: String): Flow<LiveQueryEvent<BrowsePage>> =
        when (val id = checkNotNull(BrowseId.parse(parentId))) {
            is BrowseId.Album -> {
                library.albumDetails(id.albumId).mapBrowse { detail ->
                    detail
                        ?.let(::primaryRelease)
                        ?.let { release ->
                            flatTracks(release).mapIndexed { index, track -> nodes.track(release, track, index) }
                        }.orEmpty()
                }
            }

            is BrowseId.Composer -> {
                library.composerDetails(id.artistId).mapBrowse { detail ->
                    detail
                        ?.let {
                            it.workGroups
                                .flatMap { group -> listOfNotNull(group.parent) + group.works }
                                .map(nodes::work) +
                                it.unlinkedReleaseRoles.map { role -> nodes.album(role.albumId, role.albumTitle, null) }
                        }.orEmpty()
                }
            }

            is BrowseId.Work -> {
                library.workDetails(id.workId).mapBrowse { detail ->
                    detail
                        ?.let {
                            it.childWorks.map(nodes::work) +
                                it.releases.map { release ->
                                    nodes.album(release.albumId, release.albumTitle, release.cover)
                                }
                        }.orEmpty()
                }
            }

            else -> {
                error("$parentId has no fixed parent query")
            }
        }

    private fun checkOpen() {
        synchronized(lock) {
            if (closed) throw treeClosedError()
        }
    }

    private companion object {
        const val ROOT_TITLE = "bae"
        val ALBUM_SORT = BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING)
        val COMPOSER_SORT = BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING)
        const val MAXIMUM_IMPLICIT_PARENT_INTERESTS = 12
    }
}

private typealias OptionalLiveQuery<Value> = Flow<LiveQueryEvent<Value?>>
private typealias BrowsePageQuery = Flow<LiveQueryEvent<BrowsePage>>

private fun <Value> OptionalLiveQuery<Value>.mapBrowse(rows: (Value?) -> List<MediaItem>): BrowsePageQuery =
    map { event ->
        event.mapValue { value -> rows(value).let { BrowsePage(it, it.size) } }
    }

private fun primaryRelease(detail: BridgeAlbumDetail): BridgeRelease? =
    detail.releases.firstOrNull { it.id == detail.album.primaryReleaseId } ?: detail.releases.firstOrNull()

private fun flatTracks(release: BridgeRelease): List<BridgeTrack> = release.trackGroups.flatMap { it.tracks }

internal fun windowEvicted(): BridgeException =
    BridgeException.Diagnostic(BridgeErrorCategory.INTERNAL, "browse window was replaced by a newer request")

internal fun treeClosedError(): BridgeException = browseDiagnostic(TREE_CLOSED_MESSAGE)

private fun parentInterestEnded(): BridgeException =
    BridgeException.Diagnostic(BridgeErrorCategory.INTERNAL, "library browse parent interest ended")

private fun searchInterestEnded(): BridgeException =
    BridgeException.Diagnostic(BridgeErrorCategory.INTERNAL, "library browse search interest ended")

private fun browseDiagnostic(message: String): BridgeException = Diagnostic(BridgeErrorCategory.INTERNAL, message)
