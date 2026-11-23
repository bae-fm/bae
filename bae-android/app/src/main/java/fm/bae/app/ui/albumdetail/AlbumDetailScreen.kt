package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.audioLineText
import fm.bae.app.data.LiveQueryState
import fm.bae.app.data.answer
import fm.bae.app.displayError
import fm.bae.app.text
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.PageDeletion
import fm.bae.app.ui.components.PageDeletionShown
import fm.bae.app.ui.components.rememberPageDeletion
import fm.bae.app.ui.library.AndroidDetailScreen
import fm.bae.app.ui.library.TrackPlaylistAdd
import fm.bae.app.ui.library.TrackPlaylistAddShown
import fm.bae.app.ui.library.rememberTrackPlaylistAdd
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.runTrackQueueCommand
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumDetailRequest
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRatingTarget
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.bridgeAudioChannelsKey
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
    val heart = rememberAlbumHeart(session, albumId)
    // The album query is one shared slot that still holds the previous album
    // right after navigation, so ignore any value for another album.
    val detail = query.answer?.takeIf { it.album.id == albumId }
    var selectedReleaseId by remember(albumId, initialReleaseId) { mutableStateOf(initialReleaseId) }
    val deletion =
        rememberPageDeletion<BridgeAlbum>(session, "delete album $albumId", rememberScreenScope(), onBack) {
            session.appHandle.deleteAlbum(albumId)
        }
    val adding = rememberTrackPlaylistAdd(session)
    val unpin = rememberReleaseUnpin(session)
    val stars = detail?.rating as? BridgeAlbumRating.Stars
    var rating by remember(albumId, stars != null) { mutableStateOf(false) }
    ActivateAlbumQuery(session, albumId, selectedReleaseId)

    val loadError = (query as? LiveQueryState.Failed)?.error?.let(LocalContext.current::displayError)
    val loaded =
        detail?.let {
            LoadedAlbum(
                it,
                buildAlbumDetailCallbacks(session, it.shownRelease, adding, onOpenWork) { selectedReleaseId = it },
                heart.rememberShownRating(it.rating),
            )
        }
    AndroidDetailScreen(
        onBack = onBack,
        backLabel = backLabel,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
        actions = {
            if (loaded != null) {
                AlbumBarActions(
                    session = session,
                    detail = loaded.detail,
                    callbacks = loaded.callbacks,
                    deletion = deletion,
                    unpin = unpin,
                    shownRating = { loaded.shownRating.value },
                    onSetHearted = heart::choose,
                    onRate = { rating = true },
                )
            }
        },
    ) {
        AlbumPageOverlays(session, deletion, unpin, adding)
        if (rating && stars != null && loaded != null) {
            AlbumRatingSheet(rating = stars, onRate = loaded.callbacks.onRateAlbum, onDismiss = { rating = false })
        }
        AlbumPageBody(session, loaded, albumDetailPlaceholder(query, loadError))
    }
}

/** The album page below its bar: the [loaded] album, or the [placeholder] while there is none. */
@Composable
private fun ColumnScope.AlbumPageBody(
    session: OpenLibrary,
    loaded: LoadedAlbum?,
    placeholder: AlbumDetailPlaceholder,
) {
    Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
        if (loaded == null) {
            AlbumDetailPlaceholderBox(placeholder = placeholder, onRetry = session.libraryQueries.album::retry)
        } else {
            AlbumLoaded(session, loaded)
        }
    }
}

private class LoadedAlbum(
    val detail: BridgeAlbumDetail,
    val callbacks: AlbumDetailCallbacks,
    val shownRating: State<BridgeAlbumRating>,
)

/** The album page's content for the [loaded] album. */
@Composable
private fun AlbumLoaded(
    session: OpenLibrary,
    loaded: LoadedAlbum,
) {
    val nowPlaying by session.playback.nowPlaying.collectAsState()
    val isPlaying by session.playback.isPlaying.collectAsState()
    AlbumDetailPage(
        detail = loaded.detail,
        cover = loaded.detail.shownRelease.cover,
        playback = AlbumPlaybackState(nowPlaying?.trackId, isPlaying),
        callbacks = loaded.callbacks,
        releaseDownloadControl = { rel -> ReleaseTransferLines(session = session, release = rel) },
        shownRating = { loaded.shownRating.value },
    )
}

/**
 * The page activates the album query again with each release the person picks, and deactivates
 * it when the page closes or shows another album.
 */
@Composable
private fun ActivateAlbumQuery(
    session: OpenLibrary,
    albumId: String,
    selectedReleaseId: String?,
) {
    val request = BridgeAlbumDetailRequest(albumId = albumId, chosenReleaseId = selectedReleaseId)
    LaunchedEffect(request, session) { session.libraryQueries.album.activate(request) }
    DisposableEffect(albumId, session) {
        onDispose { session.libraryQueries.album.deactivate(request) }
    }
}

/** What the page opens over its content: the delete and unpin confirmations, the playlist choice and their errors. */
@Composable
private fun AlbumPageOverlays(
    session: OpenLibrary,
    deletion: PageDeletion<BridgeAlbum>,
    unpin: ReleaseUnpin,
    adding: TrackPlaylistAdd,
) {
    PageDeletionShown(deletion) { _, onConfirm, onDismiss -> DeleteAlbumDialog(onConfirm, onDismiss) }
    ReleaseUnpinShown(unpin)
    TrackPlaylistAddShown(session, adding)
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

/** The album page below its bar: [detail]'s blocks in the page's padded column. */
@Composable
internal fun AlbumDetailPage(
    detail: BridgeAlbumDetail,
    cover: BridgeImageRef?,
    playback: AlbumPlaybackState,
    callbacks: AlbumDetailCallbacks,
    releaseDownloadControl: @Composable (BridgeRelease) -> Unit,
    shownRating: () -> BridgeAlbumRating,
    catalogName: (BridgeCatalog) -> String = ::bridgeCatalogName,
    audioChannelsKey: (Long) -> String? = ::bridgeAudioChannelsKey,
) {
    val context = LocalContext.current
    val shown = detail.shownRelease

    // A release only another device holds can't play here: its words and info button stand where Play would.
    val playability = shown.playability
    val unplayable = playability is BridgeReleasePlayability.OnOtherDevice
    var showWhere by remember { mutableStateOf(false) }
    if (showWhere && playability is BridgeReleasePlayability.OnOtherDevice) {
        UnplayableSheet(playability, onDismiss = { showWhere = false })
    }

    var showGallery by remember { mutableStateOf(false) }
    if (showGallery && shown.galleryItemCount > 0u) {
        AndroidLightbox(
            releaseId = shown.id,
            items = shown.galleryItems,
            count = shown.galleryItemCount,
            onDismiss = { showGallery = false },
        )
    }

    val positionColumnWidth =
        rememberPositionColumnWidth(shown.tracks.map { it.positionText })

    AndroidAlbumDetailContent {
        block {
            AndroidAlbumHeader(
                detail = detail,
                cover = cover,
                galleryItemCount = shown.galleryItemCount,
                onShowGallery = { showGallery = true },
                onShowWhere = { showWhere = true },
                shownRating = shownRating,
                actions = {
                    AndroidButton(onClick = callbacks.onPlayRelease) { Text(stringResource(R.string.play)) }
                },
            )
        }

        if (detail.showsReleaseChoices) {
            block { AndroidReleasePicker(detail, onSelect = callbacks.onSelectRelease) }
        }

        if (shown.hasRecords) {
            block { AndroidRecordLinks(shown.records, catalogName) }
        }

        // The page's transfer lines show what this device uploads of the release, even when it can't play it.
        if (!unplayable || shown.upload?.progress != null) {
            block { releaseDownloadControl(shown) }
        }
        albumTrackGroups(shown, positionColumnWidth, playback, callbacks, context)
        albumCredits(shown)
        if (shown.hasOtherFiles) {
            item { AndroidReleaseFiles(shown.otherFiles) }
        }
        shown.audioLineText(context, audioChannelsKey).takeIf { it.isNotEmpty() }?.let {
            item { AndroidAlbumAudio(it) }
        }
    }
}

/** The album's releases as chips, the shown one on the accent's tint; a tap shows that release. */
@Composable
internal fun AndroidReleasePicker(
    detail: BridgeAlbumDetail,
    onSelect: (String) -> Unit,
) {
    val context = LocalContext.current
    val colors = MaterialTheme.colorScheme
    val shape = MaterialTheme.shapes.small
    Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        detail.releases.forEachIndexed { index, rel ->
            val chosen = index == detail.shownReleaseIndex.toInt()
            val face =
                if (chosen) {
                    Modifier.background(colors.primary.copy(alpha = ThemeOpacity.tint))
                } else {
                    Modifier.border(1.dp, BaeTheme.colors.hairlineStrong, shape)
                }
            Box(
                modifier =
                    Modifier
                        .heightIn(min = RELEASE_CHIP_HEIGHT)
                        .clip(shape)
                        .then(face)
                        .selectable(selected = chosen, onClick = { onSelect(rel.id) }, role = Role.RadioButton)
                        .padding(horizontal = ThemeSpace.edge),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = rel.name.text(context),
                    style = ThemeText.androidLabel.style,
                    color = colors.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

private val RELEASE_CHIP_HEIGHT = 32.dp
