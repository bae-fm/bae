package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.creditLineText
import fm.bae.app.displayError
import fm.bae.app.durationClockLabel
import fm.bae.app.factsLine
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.PlaylistPicture
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import uniffi.bae_bridge.BridgePlaylistDetail

private val HEADER_PICTURE_SIZE = 96.dp

/** A playlist's page: its header, Play and Shuffle, then its tracks in order. */
@Composable
internal fun PlaylistScreen(
    session: OpenLibrary,
    playlistId: String,
    onBack: () -> Unit,
) {
    val query by session.libraryQueries.playlist.state
        .collectAsState()
    // The playlist query is one shared slot that still holds the previous playlist right after navigation.
    val detail = query.value?.takeIf { it.summary.id == playlistId }
    DisposableEffect(playlistId, session) {
        session.libraryQueries.playlist.activate(playlistId)
        onDispose { session.libraryQueries.playlist.deactivate(playlistId) }
    }
    val loadError = query.error?.let(LocalContext.current::displayError)
    AndroidDetailScreen(
        onBack = onBack,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
    ) {
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
                query.delivered -> {
                    Text(
                        text = stringResource(R.string.playlist_not_in_library),
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                    )
                }

                // The playlist is a local read; nothing shows until it lands.
                else -> {}
            }
        }
    }
}

/** [detail] playing through the session: a play answers as an album's does, in the session's snackbar. */
@Composable
internal fun PlaylistPage(
    session: OpenLibrary,
    detail: BridgePlaylistDetail,
) {
    val playlistId = detail.summary.id
    PlaylistContent(
        detail = detail,
        onPlay = { startPositionId, shuffle ->
            session.messages.play("play playlist $playlistId") {
                session.appHandle.playPlaylist(playlistId, startPositionId, shuffle)
            }
        },
    )
}

/**
 * The playlist's header, Play and Shuffle, and its entries; [onPlay] gets the entry to start from, if
 * any, and whether to shuffle.
 */
@Composable
internal fun PlaylistContent(
    detail: BridgePlaylistDetail,
    onPlay: (startPositionId: String?, shuffle: Boolean) -> Unit,
) {
    val context = LocalContext.current
    val summary = detail.summary
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
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
                                    context.getString(summary.sharing.labelRes()),
                                ),
                            ),
                        style = ThemeText.detail.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
        item {
            Row(
                modifier = Modifier.padding(horizontal = ThemeSpace.edge).padding(bottom = ThemeSpace.related),
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                AndroidButton(onClick = { onPlay(null, false) }) { Text(stringResource(R.string.play)) }
                AndroidButton(kind = ButtonKind.OUTLINED, onClick = { onPlay(null, true) }) {
                    Text(stringResource(R.string.shuffle))
                }
            }
        }
        items(detail.entries, key = { it.positionId }) { entry ->
            AndroidLibraryRow(
                title = entry.title,
                subtitle = context.factsLine(listOf(context.creditLineText(entry.artistLine), entry.albumTitle)),
                trailing = context.durationClockLabel(entry.durationClock),
                art = entry.cover,
                onClick = { onPlay(entry.positionId, false) },
            )
        }
    }
}
