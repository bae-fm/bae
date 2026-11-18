package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.creditLineText
import fm.bae.app.data.LiveQueryState
import fm.bae.app.data.answer
import fm.bae.app.displayError
import fm.bae.app.labelsLineText
import fm.bae.app.pressingLineText
import fm.bae.app.text
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.PageDeletionShown
import fm.bae.app.ui.components.StatusChip
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.rememberPageDeletion
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.library.AndroidDetailScreen
import fm.bae.app.ui.library.TrackPlaylistAdd
import fm.bae.app.ui.library.TrackPlaylistAddShown
import fm.bae.app.ui.library.rememberTrackPlaylistAdd
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.runTrackQueueCommand
import fm.bae.app.yearText
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumDetailRequest
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRatingTarget
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.bridgeCatalogName

internal data class AlbumPlaybackState(
    val currentTrackId: String?,
    val isPlaying: Boolean,
)

internal data class AlbumDetailCallbacks(
    val onSelectRelease: (String) -> Unit,
    val onTogglePlayPause: () -> Unit,
    val onPlayTrack: (String) -> Unit,
    val onPlayRelease: () -> Unit,
    val onShuffleRelease: () -> Unit,
    val onPlayReleaseNext: () -> Unit,
    val onAddReleaseToQueue: () -> Unit,
    val onPlayTrackNext: (String) -> Unit,
    val onAddTrackToQueue: (String) -> Unit,
    val onRateAlbum: (BridgeStars?) -> Unit,
    val onSelectWork: (String) -> Unit,
    /** The menu shows Add to playlist only when the person can make playlists; null hides it. */
    val onAddTrackToPlaylist: ((String) -> Unit)? = null,
)

/**
 * Album detail: header, a release picker when the album has several releases,
 * and the selected release's tracks grouped by side.
 */
@Composable
fun AlbumDetailScreen(
    session: OpenLibrary,
    albumId: String,
    initialReleaseId: String? = null,
    onBack: () -> Unit,
    backLabel: String,
    onOpenWork: (String) -> Unit,
) {
    val query by session.libraryQueries.album.state
        .collectAsState()
    // The album query is one shared slot that still holds the previous album
    // right after navigation, so ignore any value for another album.
    val detail = query.answer?.takeIf { it.album.id == albumId }
    val nowPlaying by session.playback.nowPlaying.collectAsState()
    val isPlaying by session.playback.isPlaying.collectAsState()
    var selectedReleaseId by remember(albumId, initialReleaseId) { mutableStateOf(initialReleaseId) }
    val deletion =
        rememberPageDeletion<BridgeAlbum>(session, "delete album $albumId", rememberScreenScope(), onBack) {
            session.appHandle.deleteAlbum(albumId)
        }
    val adding = rememberTrackPlaylistAdd(session)
    val request = BridgeAlbumDetailRequest(albumId = albumId, chosenReleaseId = selectedReleaseId)
    // The page activates the album query again with each release the person picks, and deactivates
    // it when the page closes or shows another album.
    LaunchedEffect(request, session) { session.libraryQueries.album.activate(request) }
    DisposableEffect(albumId, session) {
        onDispose { session.libraryQueries.album.deactivate(request) }
    }

    val loadError = (query as? LiveQueryState.Failed)?.error?.let(LocalContext.current::displayError)
    AndroidDetailScreen(
        onBack = onBack,
        backLabel = backLabel,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
        actions = { if (detail != null) AlbumBarActions(session, detail, deletion) },
    ) {
        PageDeletionShown(deletion) { _, onConfirm, onDismiss -> DeleteAlbumDialog(onConfirm, onDismiss) }
        TrackPlaylistAddShown(session, adding)
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            val loaded = detail
            if (loaded == null) {
                AlbumDetailPlaceholderBox(
                    placeholder = albumDetailPlaceholder(query, loadError),
                    onRetry = session.libraryQueries.album::retry,
                )
            } else {
                val release = loaded.shownRelease
                AlbumDetailContent(
                    detail = loaded,
                    cover = release.cover,
                    playback = AlbumPlaybackState(nowPlaying?.trackId, isPlaying),
                    callbacks =
                        buildAlbumDetailCallbacks(session, release, adding, onOpenWork) { selectedReleaseId = it },
                    releaseDownloadControl = { rel -> ReleaseTransferLines(session = session, release = rel) },
                )
            }
        }
    }
}

/** What the album page shows while it has no album to draw. */
internal sealed interface AlbumDetailPlaceholder {
    data object Loading : AlbumDetailPlaceholder

    /** The read failed; Retry reads it again. */
    data class Failed(
        val error: DisplayError,
    ) : AlbumDetailPlaceholder

    /** The read answered that no such album exists: it was deleted, here or on another device. */
    data object NotInLibrary : AlbumDetailPlaceholder
}

/** The placeholder for [query] while it holds no album, given its failure as [loadError]. */
internal fun albumDetailPlaceholder(
    query: LiveQueryState<BridgeAlbumDetail?>,
    loadError: DisplayError?,
): AlbumDetailPlaceholder =
    when {
        loadError != null -> AlbumDetailPlaceholder.Failed(loadError)
        query is LiveQueryState.Answered && query.value == null -> AlbumDetailPlaceholder.NotInLibrary
        else -> AlbumDetailPlaceholder.Loading
    }

/** A deleted album says so, with the top bar's back as the way out, like a missing artist or composer. */
@Composable
private fun BoxScope.AlbumDetailPlaceholderBox(
    placeholder: AlbumDetailPlaceholder,
    onRetry: () -> Unit,
) {
    when (placeholder) {
        is AlbumDetailPlaceholder.Failed -> {
            AndroidNotice(
                error = placeholder.error,
                look = NoticeLook.CENTERED,
                action = placeholder.error.retryAction(onClick = onRetry),
            )
        }

        AlbumDetailPlaceholder.NotInLibrary -> {
            Text(
                text = stringResource(R.string.album_not_in_library),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
            )
        }

        // The album is a local read; nothing shows until it lands.
        AlbumDetailPlaceholder.Loading -> {}
    }
}

/** The page's commands, each run in the session, which shows a failure as a message. */
private fun buildAlbumDetailCallbacks(
    session: OpenLibrary,
    release: BridgeRelease,
    addToPlaylist: TrackPlaylistAdd,
    onSelectWork: (String) -> Unit,
    onSelectRelease: (String) -> Unit,
): AlbumDetailCallbacks {
    fun runPlayReleaseCommand(
        operation: (BridgeRelease) -> String,
        startTrackId: String?,
        shuffled: Boolean,
    ) {
        session.messages.play(operation(release)) {
            session.appHandle.playRelease(release.id, startTrackId, shuffled)
        }
    }

    /** Adds the release to Up Next and shows a snackbar with the number of tracks the command added. */
    fun runReleaseTransportCommand(
        operation: String,
        command: suspend (String) -> UInt,
    ) {
        session.messages.perform("$operation ${release.id}") {
            session.playback.onTracksAdded(command(release.id).toInt())
        }
    }

    return AlbumDetailCallbacks(
        onSelectRelease = onSelectRelease,
        onSelectWork = onSelectWork,
        onTogglePlayPause = { session.playback.togglePlayPause() },
        onPlayTrack = { trackId ->
            runPlayReleaseCommand(
                operation = { "playRelease ${it.id} track $trackId" },
                startTrackId = trackId,
                shuffled = false,
            )
        },
        onPlayRelease = {
            runPlayReleaseCommand(
                operation = { "playRelease ${it.id}" },
                startTrackId = null,
                shuffled = false,
            )
        },
        onShuffleRelease = {
            runPlayReleaseCommand(
                operation = { "shuffleRelease ${it.id}" },
                startTrackId = null,
                shuffled = true,
            )
        },
        onPlayReleaseNext = {
            runReleaseTransportCommand("addReleaseNext", session.appHandle::addReleaseNext)
        },
        onAddReleaseToQueue = {
            runReleaseTransportCommand("addReleaseToQueue", session.appHandle::addReleaseToQueue)
        },
        onPlayTrackNext = { trackId ->
            session.runTrackQueueCommand(trackId, "addNext", session.appHandle::addNext)
        },
        onAddTrackToQueue = { trackId ->
            session.runTrackQueueCommand(trackId, "addToQueue", session.appHandle::addToQueue)
        },
        onRateAlbum = { stars ->
            session.messages.perform("setRating album ${release.albumId}") {
                session.appHandle.setRating(BridgeRatingTarget.Album(release.albumId), stars)
            }
        },
        onAddTrackToPlaylist = if (addToPlaylist.offered) addToPlaylist::ask else null,
    )
}

@Composable
internal fun AlbumDetailContent(
    detail: BridgeAlbumDetail,
    cover: BridgeImageRef?,
    playback: AlbumPlaybackState,
    callbacks: AlbumDetailCallbacks,
    releaseDownloadControl: @Composable (BridgeRelease) -> Unit,
    catalogName: (BridgeCatalog) -> String = ::bridgeCatalogName,
) {
    val album = detail.album
    val context = LocalContext.current
    val shown = detail.shownRelease

    var showGallery by remember { mutableStateOf(false) }
    if (showGallery && shown.galleryItemCount > 0u) {
        GalleryDialog(
            releaseId = shown.id,
            items = shown.galleryItems,
            count = shown.galleryItemCount,
            onDismiss = { showGallery = false },
        )
    }

    val positionColumnWidth =
        rememberPositionColumnWidth(shown.tracks.map { it.positionText })

    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.edge),
    ) {
        block {
            AndroidAlbumHeader(
                detail = detail,
                cover = cover,
                galleryItemCount = shown.galleryItemCount,
                onShowGallery = { showGallery = true },
            )
        }

        if (detail.rating.shown) {
            block { AlbumRating(rating = detail.rating, onRate = callbacks.onRateAlbum) }
        }

        if (detail.showsReleaseChoices) {
            block {
                Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                    detail.releases.forEachIndexed { index, rel ->
                        val chosen = index == detail.shownReleaseIndex.toInt()
                        StatusChip(
                            text = rel.name.text(context),
                            tone = if (chosen) StatusTone.ACCENT else StatusTone.NEUTRAL,
                            modifier =
                                Modifier.selectable(
                                    selected = chosen,
                                    onClick = { callbacks.onSelectRelease(rel.id) },
                                    role = Role.RadioButton,
                                ),
                        )
                    }
                }
            }
        }

        if (shown.hasRecords) {
            block { AlbumRecordLinks(shown.records, catalogName) }
        }

        block { releaseDownloadControl(shown) }
        block { AlbumActionButtons(callbacks) }
        albumTrackGroups(shown, positionColumnWidth, playback, callbacks, context)
        albumCredits(shown, context)
        albumFiles(shown, context)
    }
}

@Composable
internal fun AndroidAlbumHeader(
    detail: BridgeAlbumDetail,
    cover: BridgeImageRef?,
    galleryItemCount: UInt,
    onShowGallery: () -> Unit,
) {
    val album = detail.album
    val context = LocalContext.current

    Row(verticalAlignment = Alignment.Top) {
        CoverImage(
            cover = cover,
            cornerRadius = ThemeRadius.cover,
            iconPadding = ThemeSpace.page,
            modifier =
                Modifier
                    .size(140.dp)
                    .clickable(enabled = galleryItemCount > 0u, onClick = onShowGallery),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.edge))
        Column(modifier = Modifier.weight(1f)) {
            Text(text = album.title, style = ThemeText.hero.style)
            Text(
                text = LocalContext.current.creditLineText(album.artistLine),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            album.year?.let {
                Text(
                    text = LocalContext.current.yearText(it),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            // The pressing, then its labels, each on one line of its own.
            val factLines =
                listOfNotNull(
                    detail.shownRelease.pressingLineText(context),
                    detail.shownRelease.labelsLineText(context),
                ).filter { it.isNotEmpty() }
            if (factLines.isNotEmpty()) {
                Spacer(modifier = Modifier.height(ThemeSpace.inline))
                factLines.forEach { line ->
                    Text(
                        text = line,
                        style = ThemeText.detail.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

@Composable
private fun AlbumActionButtons(callbacks: AlbumDetailCallbacks) {
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
            AndroidButton(onClick = callbacks.onPlayRelease) {
                Text(stringResource(R.string.play))
            }
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = callbacks.onShuffleRelease) {
                Text(stringResource(R.string.shuffle))
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = callbacks.onPlayReleaseNext) {
                Text(stringResource(R.string.play_next))
            }
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = callbacks.onAddReleaseToQueue) {
                Text(stringResource(R.string.add_to_queue))
            }
        }
    }
}

/** One block of the page: the board spaces them 8 dp apart, while the track rows below touch. */
private fun LazyListScope.block(content: @Composable () -> Unit) {
    item { Box(modifier = Modifier.padding(bottom = ThemeSpace.related)) { content() } }
}
