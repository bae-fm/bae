package fm.bae.app.data

import android.content.Context
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.app.diagnosticDetail
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSortCriterion

internal const val BROWSER_PAGE_SIZE = 60

internal class PageError(
    val error: DisplayError,
    val onRetry: () -> Unit,
) {
    /**
     * The error for a list read that failed with [failure]. This app writes [headline], and the notice
     * shows core's diagnostic under Details. The notice offers Retry, which calls [onRetry], unless core
     * says reading again cannot fix the failure.
     */
    constructor(
        headline: String,
        failure: BridgeException,
        errors: ErrorDisplays,
        onRetry: () -> Unit,
    ) : this(
        DisplayError(
            headline,
            failure.diagnosticDetail,
            // Core shows nothing for a cancellation, so the notice still offers Retry.
            errors.display(failure)?.offersRetry != false,
        ),
        onRetry,
    )
}

/** What a library list's body draws its states from: the read still running, its failure, and how much it holds. */
internal interface BrowserPageState {
    val loading: Boolean
    val error: PageError?

    /** Whether the list's first read has returned and its count query says the list has rows. */
    val hasRows: Boolean

    /** How many rows the list has. */
    val totalCount: Int
}

internal data class AlbumBrowseParameters(
    val criterion: BridgeSortCriterion,
    val groupByArtist: Boolean,
)

internal class AlbumPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ChainBrowserPageStore<AlbumBrowseParameters, BridgeAlbumAppearance>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.library_load_more_albums_failed,
    ) {
    fun activate(
        criterion: BridgeSortCriterion,
        groupByArtist: Boolean = false,
    ) {
        activate(AlbumBrowseParameters(criterion, groupByArtist))
    }

    override fun open(parameter: AlbumBrowseParameters): ListReads<BridgeAlbumAppearance> =
        library.albumList(listOf(parameter.criterion), parameter.groupByArtist)
}

internal class ArtistPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ChainBrowserPageStore<BridgeArtistSortCriterion, BridgeArtistSummary>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.library_load_more_artists_failed,
    ) {
    override fun open(parameter: BridgeArtistSortCriterion) = library.artistList(parameter)
}

internal class ComposerPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ChainBrowserPageStore<BridgeComposerSortCriterion, BridgeComposerSummary>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.library_load_more_composers_failed,
    ) {
    override fun open(parameter: BridgeComposerSortCriterion) = library.composerList(parameter)
}

/** Holds the rows of one composer's page; this store opens a new read for each composer. */
internal class ComposerDetailRowsPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ChainBrowserPageStore<String, BridgeComposerDetailRow>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.composer_detail_load_failed,
        loadFailed = R.string.composer_detail_load_failed,
    ) {
    override fun open(parameter: String): ListReads<BridgeComposerDetailRow> = library.composerPageList(parameter)
}

/** The download queue as the Downloads screen draws it: its entries in the order queued. */
internal class DownloadPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ChainBrowserPageStore<Unit, BridgeDownloadOp>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.pinning_load_more_failed,
        loadFailed = R.string.pinning_load_failed,
    ) {
    override fun open(parameter: Unit): ListReads<BridgeDownloadOp> = library.downloadList()
}

internal class BrowserPageStores(
    library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) {
    val albums = AlbumPageStore(library, appContext, errors, scope)
    val artists = ArtistPageStore(library, appContext, errors, scope)
    val composers = ComposerPageStore(library, appContext, errors, scope)
    val composerRows = ComposerDetailRowsPageStore(library, appContext, errors, scope)
    val downloads = DownloadPageStore(library, appContext, errors, scope)
    val playlists = PlaylistListingStore(library::playlists, appContext, errors, scope)

    fun cancel() {
        albums.deactivate()
        artists.deactivate()
        composers.deactivate()
        composerRows.deactivate()
        downloads.deactivate()
        playlists.deactivate()
    }
}
