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
import androidx.compose.runtime.CompositionLocalProvider
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
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.LocalBugReports
import fm.bae.app.MissingMainRelease
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LiveQueryState
import fm.bae.app.data.LocalImageStore
import fm.bae.app.displayError
import fm.bae.app.labelsLineText
import fm.bae.app.mainRelease
import fm.bae.app.pressingLineText
import fm.bae.app.text
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.StatusChip
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.library.AndroidDetailScreen
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.yearText
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRelease

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
) {
    val query by session.libraryQueries.album.state
        .collectAsState()
    // The album query is one shared slot that still holds the previous album
    // right after navigation, so ignore any value for another album.
    val detail = query.value?.takeIf { it.album.id == albumId }
    val nowPlaying by session.playback.nowPlaying.collectAsState()
    val isPlaying by session.playback.isPlaying.collectAsState()
    var selectedReleaseId by remember(albumId, initialReleaseId) { mutableStateOf(initialReleaseId) }
    val appContext = LocalContext.current
    val scope = rememberScreenScope()
    val deletion = rememberAlbumDeletion(session, albumId, scope, onDeleted = onBack)

    DisposableEffect(albumId, session) {
        session.libraryQueries.album.activate(albumId)
        onDispose {
            session.libraryQueries.album.deactivate(albumId)
        }
    }

    val loadError = query.error?.let(appContext::displayError)

    AndroidDetailScreen(
        onBack = onBack,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
        actions = {
            if (detail != null) {
                AlbumOptionsMenu(onDelete = deletion::ask)
            }
        },
    ) {
        AlbumDeletionShown(deletion)
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            val loaded = detail
            if (loaded == null) {
                AlbumDetailPlaceholderBox(
                    placeholder = albumDetailPlaceholder(query, loadError),
                    onRetry = session.libraryQueries.album::retry,
                )
            } else {
                loadError?.let {
                    AndroidNotice(
                        error = it,
                        modifier = Modifier.align(Alignment.TopCenter).padding(ThemeSpace.related),
                    )
                }
                // Derived rather than stored, so a release list that arrives
                // later needs no correction: the one chosen, else the main one.
                val release = loaded.releases.firstOrNull { it.id == selectedReleaseId } ?: loaded.mainRelease()
                if (release == null) {
                    MissingMainReleaseNotice(loaded)
                } else {
                    AlbumDetailContent(
                        detail = loaded,
                        selectedRelease = release,
                        cover = release.cover,
                        playback = AlbumPlaybackState(nowPlaying?.trackId, isPlaying),
                        callbacks = buildAlbumDetailCallbacks(session, release, scope) { selectedReleaseId = it },
                        releaseDownloadControl = { rel -> ReleaseTransferLines(session = session, release = rel) },
                    )
                }
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

/** The placeholder for [query] before it holds an album, given its failure as [loadError]. */
internal fun albumDetailPlaceholder(
    query: LiveQueryState<BridgeAlbumDetail>,
    loadError: DisplayError?,
): AlbumDetailPlaceholder =
    when {
        loadError != null -> AlbumDetailPlaceholder.Failed(loadError)
        query.delivered && query.value == null -> AlbumDetailPlaceholder.NotInLibrary
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

/**
 * An album whose named main release isn't among its releases: a bug, reported
 * once per detail, and "Something went wrong" where the album would be.
 */
@Composable
private fun BoxScope.MissingMainReleaseNotice(detail: BridgeAlbumDetail) {
    val bugs = LocalBugReports.current
    LaunchedEffect(detail) { bugs.reportShownInPlace("show album ${detail.album.id}", MissingMainRelease(detail)) }
    ErrorText(
        message = LocalContext.current.coreString("core.error.category.internal"),
        modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
    )
}

/**
 * The page's commands, each run in [scope], whose bug handler reports one that
 * fails: these calls fail only if this device's library is broken, never in
 * normal use.
 */
private fun buildAlbumDetailCallbacks(
    session: OpenLibrary,
    release: BridgeRelease,
    scope: CoroutineScope,
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

    fun runReleaseTransportCommand(
        operation: String,
        command: (String) -> Unit,
    ) {
        scope.launch(CoroutineName("$operation ${release.id}")) {
            command(release.id)
        }
    }

    fun runTrackTransportCommand(
        trackId: String,
        operation: String,
        command: (List<String>) -> Unit,
    ) {
        scope.launch(CoroutineName("$operation $trackId")) {
            command(listOf(trackId))
        }
    }

    return AlbumDetailCallbacks(
        onSelectRelease = onSelectRelease,
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
            runTrackTransportCommand(trackId, "addNext", session.appHandle::addNext)
        },
        onAddTrackToQueue = { trackId ->
            runTrackTransportCommand(trackId, "addToQueue", session.appHandle::addToQueue)
        },
    )
}

@Composable
internal fun AlbumDetailContent(
    detail: BridgeAlbumDetail,
    selectedRelease: BridgeRelease,
    cover: BridgeImageRef?,
    playback: AlbumPlaybackState,
    callbacks: AlbumDetailCallbacks,
    releaseDownloadControl: @Composable (BridgeRelease) -> Unit,
) {
    val album = detail.album
    val context = LocalContext.current

    var showGallery by remember { mutableStateOf(false) }
    val galleryItems = selectedRelease.galleryItems
    if (showGallery && galleryItems.isNotEmpty()) {
        GalleryDialog(
            releaseId = selectedRelease.id,
            items = galleryItems,
            onDismiss = { showGallery = false },
        )
    }

    val positionColumnWidth =
        rememberPositionColumnWidth(selectedRelease.tracks.map { it.positionText })

    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.edge),
    ) {
        block {
            AndroidAlbumHeader(
                detail = detail,
                selectedRelease = selectedRelease,
                cover = cover,
                galleryItems = galleryItems,
                onShowGallery = { showGallery = true },
            )
        }

        if (detail.releases.size > 1) {
            block {
                Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                    detail.releases.forEach { rel ->
                        val chosen = rel.id == selectedRelease.id
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

        block { releaseDownloadControl(selectedRelease) }
        block { AlbumActionButtons(callbacks) }
        albumTrackGroups(selectedRelease, positionColumnWidth, playback, callbacks, context)
    }
}

@Composable
internal fun AndroidAlbumHeader(
    detail: BridgeAlbumDetail,
    selectedRelease: BridgeRelease,
    cover: BridgeImageRef?,
    galleryItems: List<uniffi.bae_bridge.BridgeGalleryItem>,
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
                    .clickable(enabled = galleryItems.isNotEmpty(), onClick = onShowGallery),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.edge))
        Column(modifier = Modifier.weight(1f)) {
            Text(text = album.title, style = ThemeText.hero.style)
            Text(
                text = album.artistNames,
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
                    selectedRelease.pressingLineText(context),
                    selectedRelease.labelsLineText(context),
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

@Preview(showBackground = true)
@Composable
private fun AlbumDetailHeaderPreview() {
    PreviewTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            AndroidAlbumHeader(
                detail = PreviewData.albumDetail(),
                selectedRelease = PreviewData.release(),
                cover = PreviewData.imageRef(),
                galleryItems = emptyList(),
                onShowGallery = {},
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun AlbumActionButtonsPreview() {
    PreviewTheme {
        AlbumActionButtons(callbacks = inertAlbumDetailCallbacks())
    }
}

/** Inert callbacks for rendering the album detail body without a live session. */
internal fun inertAlbumDetailCallbacks(): AlbumDetailCallbacks =
    AlbumDetailCallbacks(
        onSelectRelease = {},
        onTogglePlayPause = {},
        onPlayTrack = {},
        onPlayRelease = {},
        onShuffleRelease = {},
        onPlayReleaseNext = {},
        onAddReleaseToQueue = {},
        onPlayTrackNext = {},
        onAddTrackToQueue = {},
    )
