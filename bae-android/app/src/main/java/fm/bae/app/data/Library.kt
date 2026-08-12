package fm.bae.app.data

import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.AlbumPageCallback
import uniffi.bae_bridge.AlbumDetailCallback
import uniffi.bae_bridge.ArtistPageCallback
import uniffi.bae_bridge.ArtistDetailCallback
import uniffi.bae_bridge.ComposerPageCallback
import uniffi.bae_bridge.ComposerDetailCallback
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumPage
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerPage
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.LibrarySearchCallback
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.ReleaseDetailCallback
import uniffi.bae_bridge.WorkDetailCallback
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow

private val logger = fm.bae.app.BaeLogger("bae.Library")

/**
 * Narrow projection of [AppHandle] for library browse and detail reads. The
 * page/detail calls suspend across the bridge; callers invoke them from their
 * existing coroutine. Image bytes are not here — every image in the app resolves
 * through [fm.bae.app.data.ImageStore], which owns their caching too.
 * Mirrors the macOS `Library` domain service.
 */
class Library(
    private val handle: AppHandle,
    private val onUnhandledError: (BridgeException) -> Unit = {
        logger.error("library live query failed", it)
    },
) {
    fun subscribeAlbumPage(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
        callback: AlbumPageCallback,
    ): LiveSubscription = handle.subscribeAlbumPage(sortCriteria, offset, limit, callback)

    fun albumPages(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
    ): Flow<BridgeAlbumPage> =
        callbackFlow {
            val subscription =
                subscribeAlbumPage(
                    sortCriteria,
                    offset,
                    limit,
                    object : AlbumPageCallback {
                        override fun onValue(value: BridgeAlbumPage) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onUnhandledError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    fun albumDetails(
        albumId: String,
        onError: (BridgeException) -> Unit = onUnhandledError,
    ): Flow<BridgeAlbumDetail?> =
        callbackFlow {
            val subscription =
                handle.subscribeAlbumDetail(
                    albumId,
                    object : AlbumDetailCallback {
                        override fun onValue(value: BridgeAlbumDetail?) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    fun subscribeComposerPage(
        sortCriterion: BridgeComposerSortCriterion,
        offset: ULong,
        limit: ULong,
        callback: ComposerPageCallback,
    ): LiveSubscription = handle.subscribeComposerPage(listOf(sortCriterion), offset, limit, callback)

    fun composerPages(
        sortCriterion: BridgeComposerSortCriterion,
        offset: ULong,
        limit: ULong,
    ): Flow<BridgeComposerPage> =
        callbackFlow {
            val subscription =
                subscribeComposerPage(
                    sortCriterion,
                    offset,
                    limit,
                    object : ComposerPageCallback {
                        override fun onValue(value: BridgeComposerPage) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onUnhandledError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    fun composerDetails(
        artistId: String,
        onError: (BridgeException) -> Unit = onUnhandledError,
    ): Flow<BridgeComposerDetail?> =
        callbackFlow {
            val subscription =
                handle.subscribeComposerDetail(
                    artistId,
                    object : ComposerDetailCallback {
                        override fun onValue(value: BridgeComposerDetail?) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    fun subscribeArtistPage(
        sortCriterion: BridgeArtistSortCriterion,
        offset: ULong,
        limit: ULong,
        callback: ArtistPageCallback,
    ): LiveSubscription = handle.subscribeArtistPage(listOf(sortCriterion), offset, limit, callback)

    fun artistDetails(
        artistId: String,
        onError: (BridgeException) -> Unit = onUnhandledError,
    ): Flow<BridgeArtistDetail?> =
        callbackFlow {
            val subscription =
                handle.subscribeArtistDetail(
                    artistId,
                    object : ArtistDetailCallback {
                        override fun onValue(value: BridgeArtistDetail?) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    fun workDetails(
        workId: String,
        onError: (BridgeException) -> Unit = onUnhandledError,
    ): Flow<BridgeWorkDetail?> =
        callbackFlow {
            val subscription =
                handle.subscribeWorkDetail(
                    workId,
                    object : WorkDetailCallback {
                        override fun onValue(value: BridgeWorkDetail?) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    fun releaseDetails(releaseId: String): Flow<BridgeRelease?> =
        callbackFlow {
            val subscription =
                handle.subscribeReleaseDetail(
                    releaseId,
                    object : ReleaseDetailCallback {
                        override fun onValue(value: BridgeRelease?) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onUnhandledError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    /**
     * Search albums and tracks by free-text query. Suspends: the bridge call is
     * async, so callers invoke it directly from a coroutine.
     */
    fun searchResults(
        query: String,
        onError: (BridgeException) -> Unit = onUnhandledError,
    ): Flow<BridgeSearchResults> =
        callbackFlow {
            val subscription =
                handle.subscribeLibrarySearch(
                    query,
                    object : LibrarySearchCallback {
                        override fun onValue(value: BridgeSearchResults) {
                            trySend(value)
                        }

                        override fun onError(error: BridgeException) {
                            onError(error)
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }
}
