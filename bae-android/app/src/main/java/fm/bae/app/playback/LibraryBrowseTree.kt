package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import fm.bae.app.BaeLogger
import fm.bae.app.data.Library
import fm.bae.app.data.LibrarySearch
import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
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
import uniffi.bae_bridge.BridgeLibrarySearchSnapshot
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

/** A search's album count, for the listener of the request it answers; given outside the lock. */
private class SearchNotice(
    private val listener: (Int) -> Unit,
    private val count: Int,
) {
    fun give() = listener(count)
}

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
    /**
     * One controller's search: one live query for as long as the controller is connected, pointed
     * at each phrase it searches. State is guarded by the tree's lock; only values answering the
     * newest phrase's request are kept or heard.
     */
    private inner class OwnerSearch(
        private val search: LibrarySearch,
    ) {
        /** The phrase the newest request searches, as the controller sent it. */
        var query: String = ""
            private set
        private var listener: (Int) -> Unit = {}
        private var revision: ULong = 0u
        private var latest: LiveQueryEvent<BridgeSearchResults>? = null
        private var answered = CompletableDeferred<Unit>()
        private var lastCount: Int? = null
        private var reader: Job? = null

        /** The newest value the query delivered, whichever request it answers: one can land while
         *  `setQuery` is still returning the revision it will carry. */
        private var delivered: BridgeLibrarySearchSnapshot? = null

        /**
         * Search [phrase] from now on; call under the tree's lock. Returns the request's revision,
         * and the notice to give when its answer already landed.
         */
        fun point(
            phrase: String,
            onResultsChanged: (Int) -> Unit,
        ): Pair<ULong, SearchNotice?> {
            query = phrase
            listener = onResultsChanged
            latest = null
            lastCount = null
            answered.complete(Unit)
            answered = CompletableDeferred()
            revision = search.setQuery(phrase)
            return revision to delivered?.takeIf { it.requestRevision == revision }?.let(::accept)
        }

        private fun accept(value: BridgeLibrarySearchSnapshot): SearchNotice {
            latest = LiveQueryEvent.Value(value.results)
            lastCount = value.results.albums.size
            answered.complete(Unit)
            return SearchNotice(listener, value.results.albums.size)
        }

        /** The newest request's results once they arrive, or its error. */
        suspend fun results(): BridgeSearchResults {
            while (true) {
                val (event, waiting) = synchronized(lock) { latest to answered }
                when (event) {
                    is LiveQueryEvent.Value -> return event.value
                    is LiveQueryEvent.Error -> throw event.error
                    null -> waiting.await()
                }
                synchronized(lock) { if (!retained) throw searchInterestEnded() }
            }
        }

        private val retained: Boolean
            get() = searchesByOwner.values.any { it === this }

        /** Whether [requested] is still the newest request of this controller's search. */
        fun isCurrent(requested: ULong): Boolean = retained && requested != 0uL && revision == requested

        fun start() {
            reader = scope.launch { read() }
        }

        suspend fun close() {
            synchronized(lock) { answered.complete(Unit) }
            reader?.cancelAndJoin()
            search.release()
        }

        private suspend fun read() {
            while (true) {
                val notice =
                    runCatching { search.next() }.fold(
                        onSuccess = { value ->
                            synchronized(lock) {
                                delivered = value
                                if (isCurrent(value.requestRevision)) accept(value) else null
                            }
                        },
                        onFailure = { error ->
                            if (error !is BridgeException) throw error
                            if (error is BridgeException.Cancelled) return
                            onQueryError(error)
                            synchronized(lock) {
                                if (!isCurrent(revision)) return@synchronized null
                                latest = LiveQueryEvent.Error(error)
                                answered.complete(Unit)
                                lastCount?.let { SearchNotice(listener, it) }
                            }
                        },
                    )
                notice?.give()
            }
        }
    }

    private val nodes = BrowseNodeFactory(artworkUri)
    val root: MediaItem = nodes.browsable(BrowseId.Root, ROOT_TITLE, null, MediaMetadata.MEDIA_TYPE_FOLDER_MIXED)
    private val lock = Any()
    private val parentsByOwner = mutableMapOf<Owner, ParentInterests>()
    private val fixedParents = mutableMapOf<String, FixedProjection<BrowsePage>>()
    private val searchesByOwner = mutableMapOf<Owner, OwnerSearch>()
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
                    flatTracks(release).firstOrNull { it.id == id.trackId }?.let { nodes.track(release, it) }
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
        val search = synchronized(lock) { searchesByOwner.values.firstOrNull { it.query == query } }
        val results = search?.results() ?: spokenSearches.value(query)
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

    /**
     * Point [owner]'s search at [query], opening it on the controller's first search, and wait for
     * its first answer. Later values for this query reach [onResultsChanged] with their album count.
     */
    suspend fun subscribeSearch(
        owner: Owner,
        query: String,
        onResultsChanged: (Int) -> Unit,
    ) {
        checkOpen()
        var opened: OwnerSearch? = null
        val search =
            synchronized(lock) {
                if (closed) throw treeClosedError()
                searchesByOwner.getOrPut(owner) { OwnerSearch(library.librarySearch()).also { opened = it } }
            }
        opened?.start()
        val (requested, answered) = synchronized(lock) { search.point(query, onResultsChanged) }
        answered?.give()
        search.results()
        if (!synchronized(lock) { search.isCurrent(requested) }) throw searchInterestEnded()
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
            search?.close()
            parents.forEach { closeParentIfUnused(it) }
        }

    suspend fun searchTopPlayable(query: String): BrowseId.Track? {
        checkOpen()
        val results = spokenSearches.value(query)
        val firstTrack = results.tracks.firstOrNull()
        val firstAlbum = results.albums.firstOrNull()
        return when {
            firstTrack != null -> {
                BrowseId.Track(firstTrack.releaseId, firstTrack.id)
            }

            firstAlbum != null -> {
                albumDetails.value(firstAlbum.id)?.let(::primaryRelease)?.let { release ->
                    flatTracks(release).firstOrNull()?.let { BrowseId.Track(release.id, it.id) }
                }
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
        state.third.forEach { it.close() }
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
                            flatTracks(release).map { track -> nodes.track(release, track) }
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
    BridgeException.Diagnostic(BridgeErrorCategory.Internal, "browse window was replaced by a newer request")

internal fun treeClosedError(): BridgeException = browseDiagnostic(TREE_CLOSED_MESSAGE)

private fun parentInterestEnded(): BridgeException =
    BridgeException.Diagnostic(BridgeErrorCategory.Internal, "library browse parent interest ended")

private fun searchInterestEnded(): BridgeException =
    BridgeException.Diagnostic(BridgeErrorCategory.Internal, "library browse search interest ended")

private fun browseDiagnostic(message: String): BridgeException = Diagnostic(BridgeErrorCategory.Internal, message)
