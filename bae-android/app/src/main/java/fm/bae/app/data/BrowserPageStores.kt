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
import uniffi.bae_bridge.BridgeHistoryPlay
import uniffi.bae_bridge.BridgeSortCriterion

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

/** What a library list's body draws, from its reads' latest answers. */
internal sealed interface ListPageState {
    /** The list's count has not answered yet. */
    data object Waiting : ListPageState

    /** A read failed, and its failure stands in the rows' place until the read answers again. */
    data class Failed(
        val error: PageError,
    ) : ListPageState

    /** The list's count answered: the list has [rows] rows, and none means it is empty. */
    data class Counted(
        val rows: Int,
    ) : ListPageState
}

/** A library list whose body draws its [state]. */
internal interface BrowserPageState {
    val state: ListPageState
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
) : ListBrowserPageStore<AlbumBrowseParameters, BridgeAlbumAppearance>(
        appContext,
        errors,
        scope,
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
) : ListBrowserPageStore<BridgeArtistSortCriterion, BridgeArtistSummary>(
        appContext,
        errors,
        scope,
    ) {
    override fun open(parameter: BridgeArtistSortCriterion) = library.artistList(parameter)
}

internal class ComposerPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ListBrowserPageStore<BridgeComposerSortCriterion, BridgeComposerSummary>(
        appContext,
        errors,
        scope,
    ) {
    override fun open(parameter: BridgeComposerSortCriterion) = library.composerList(parameter)
}

/** Holds the rows of one composer's page; this store opens a new read for each composer. */
internal class ComposerDetailRowsPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ListBrowserPageStore<String, BridgeComposerDetailRow>(
        appContext,
        errors,
        scope,
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
) : ListBrowserPageStore<Unit, BridgeDownloadOp>(
        appContext,
        errors,
        scope,
        loadFailed = R.string.pinning_load_failed,
    ) {
    override fun open(parameter: Unit): ListReads<BridgeDownloadOp> = library.downloadList()
}

/** Whose plays History lists: everyone's with a null [person], else those of the person with that member id. */
internal data class HistoryOf(
    val person: String?,
)

/** History's plays, newest first, under the days they were played. */
internal class HistoryPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ListBrowserPageStore<HistoryOf, BridgeHistoryPlay>(
        appContext,
        errors,
        scope,
    ) {
    override fun open(parameter: HistoryOf): ListReads<BridgeHistoryPlay> = library.historyList(parameter.person)
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
    val history = HistoryPageStore(library, appContext, errors, scope)
    val playlists = PlaylistListingStore(library::playlists, appContext, errors, scope)

    fun cancel() {
        albums.deactivate()
        artists.deactivate()
        composers.deactivate()
        composerRows.deactivate()
        downloads.deactivate()
        history.deactivate()
        playlists.deactivate()
    }
}
