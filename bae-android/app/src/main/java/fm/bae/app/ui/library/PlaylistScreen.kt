package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.LiveQueryState
import fm.bae.app.data.answer
import fm.bae.app.displayError
import fm.bae.app.factsLine
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidContentUnavailable
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.PageDeletion
import fm.bae.app.ui.components.PageDeletionShown
import fm.bae.app.ui.components.PlaylistPicture
import fm.bae.app.ui.components.rememberPageDeletion
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.runTrackQueueCommand
import sh.calvin.reorderable.rememberReorderableLazyListState
import uniffi.bae_bridge.BridgePlaylistDetail
import uniffi.bae_bridge.BridgePlaylistSummary

private val HEADER_PICTURE_SIZE = 96.dp

/** A playlist's page: its header with Play, then its tracks in order. */
@Composable
internal fun PlaylistScreen(
    session: OpenLibrary,
    playlistId: String,
    onBack: () -> Unit,
    backLabel: String,
) {
    val query by session.libraryQueries.playlist.state
        .collectAsState()
    // The playlist query is one shared slot that still holds the previous playlist right after navigation.
    val detail = query.answer?.takeIf { it.summary.id == playlistId }
    DisposableEffect(playlistId, session) {
        session.libraryQueries.playlist.activate(playlistId)
        onDispose { session.libraryQueries.playlist.deactivate(playlistId) }
    }
    val loadError = (query as? LiveQueryState.Failed)?.error?.let(LocalContext.current::displayError)
    val scope = rememberScreenScope()
    val deletion =
        rememberPageDeletion<BridgePlaylistSummary>(session, "delete playlist $playlistId", scope, onBack) {
            session.appHandle.deletePlaylist(playlistId)
        }
    var editing by remember { mutableStateOf(false) }
    AndroidDetailScreen(
        onBack = onBack,
        backLabel = backLabel,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
        actions = { if (detail != null) PlaylistBarMenu(session, detail.summary, deletion) { editing = true } },
    ) {
        PageDeletionShown(deletion) { playlist, onConfirm, onDismiss ->
            DeletePlaylistDialog(name = playlist.name, onConfirm = onConfirm, onDismiss = onDismiss)
        }
        if (editing && detail != null) {
            EditPlaylistDialog(session = session, playlist = detail.summary, onDismiss = { editing = false })
        }
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            when {
                detail != null -> {
                    PlaylistPage(session = session, detail = detail)
                }

                loadError != null -> {
                    AndroidNotice(
                        error = loadError,
                        look = NoticeLook.CENTERED,
                        action = loadError.retryAction(onClick = session.libraryQueries.playlist::retry),
                    )
                }

                // The playlist was deleted, here or on another device.
                query is LiveQueryState.Answered -> {
                    AndroidContentUnavailable(
                        message = stringResource(R.string.playlist_not_in_library),
                        modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                    )
                }

                // The playlist is a local read; nothing shows until it lands.
                else -> {}
            }
        }
    }
}

/** The playlist's menu in the page's bar: Shuffle for everyone, and Edit and Delete when the person owns it. */
@Composable
private fun PlaylistBarMenu(
    session: OpenLibrary,
    summary: BridgePlaylistSummary,
    deletion: PageDeletion<BridgePlaylistSummary>,
    onEdit: () -> Unit,
) {
    val owner = if (summary.yours) PlaylistOwnerActions(onEdit, onDelete = { deletion.ask(summary) }) else null
    PlaylistOptionsMenu(onShuffle = { session.playPlaylist(summary.id, null, shuffle = true) }, owner = owner)
}

/**
 * Plays the playlist [playlistId] from [startPositionId] or its start: a play answers as an album's
 * does, in the session's snackbar.
 */
private fun OpenLibrary.playPlaylist(
    playlistId: String,
    startPositionId: String?,
    shuffle: Boolean,
) {
    messages.play("play playlist $playlistId") { appHandle.playPlaylist(playlistId, startPositionId, shuffle) }
}

/** [detail] playing through the session: a play answers as an album's does, in the session's snackbar. */
@Composable
internal fun PlaylistPage(
    session: OpenLibrary,
    detail: BridgePlaylistDetail,
) {
    val playlistId = detail.summary.id
    val addToPlaylist = rememberTrackPlaylistAdd(session)
    TrackPlaylistAddShown(session, addToPlaylist)
    PlaylistContent(
        detail = detail,
        actions =
            PlaylistEntryActions(
                onPlay = { startPositionId, shuffle -> session.playPlaylist(playlistId, startPositionId, shuffle) },
                onPlayNext = { trackId ->
                    session.runTrackQueueCommand(trackId, "addNext", session.appHandle::addNext)
                },
                onAddToQueue = { trackId ->
                    session.runTrackQueueCommand(trackId, "addToQueue", session.appHandle::addToQueue)
                },
                onAddToPlaylist = if (addToPlaylist.offered) addToPlaylist::ask else null,
                onRemove = { positionId ->
                    session.messages.perform("remove $positionId from playlist $playlistId") {
                        session.appHandle.removePlaylistEntry(playlistId, positionId)
                    }
                },
                onMove = { positionId, place, onReturned ->
                    session.messages.perform("move $positionId to $place in playlist $playlistId") {
                        try {
                            session.appHandle.movePlaylistEntry(playlistId, positionId, place)
                        } finally {
                            onReturned()
                        }
                    }
                },
            ),
    )
}

/**
 * The playlist's header with Play, and its entries, with what each entry's menu and the
 * playlist's own buttons do as [actions] says.
 */
@Composable
internal fun PlaylistContent(
    detail: BridgePlaylistDetail,
    actions: PlaylistEntryActions,
) {
    val context = LocalContext.current
    val summary = detail.summary
    val reorder = rememberPlaylistReorder(detail, actions.onMove)
    val latestReorder by rememberUpdatedState(reorder)
    val listState = rememberLazyListState()
    val reorderState =
        rememberReorderableLazyListState(listState) { from, to ->
            // A step of the drag moves the rows on screen only; letting go writes the one move.
            (to.key as? String)?.let { onto -> latestReorder.step(onto, down = to.index > from.index) }
        }
    LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
        item {
            AndroidPlaylistHeader(summary = summary, onPlay = { actions.onPlay(null, false) })
        }
        playlistEntries(detail, actions, reorder, reorderState, context)
    }
}

/** A playlist page's head: its picture, then its name and facts with Play under them. */
@Composable
internal fun AndroidPlaylistHeader(
    summary: BridgePlaylistSummary,
    onPlay: () -> Unit,
) {
    val context = LocalContext.current
    Row(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PlaylistPicture(
            cover = summary.cover,
            cornerRadius = ThemeRadius.cover,
            modifier = Modifier.size(HEADER_PICTURE_SIZE),
        )
        Column {
            Text(text = summary.name, style = ThemeText.title.style)
            Text(
                text =
                    context.factsLine(
                        listOf(
                            summary.owner.name,
                            context.coreString(summary.trackCount),
                            stringResource(summary.sharing.labelRes()),
                        ),
                    ),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(modifier = Modifier.height(ThemeSpace.group))
            AndroidButton(onClick = onPlay) { Text(stringResource(R.string.play)) }
        }
    }
}
