package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.castEndedFailure
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.color
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.bridgeNextRepeatMode

/**
 * Persistent now-playing bar, hidden until something is loaded or playback
 * can't be read. A failed read shows in place of the track, which is no
 * longer known to be current; a cast whose link was lost says so under it
 * until playback moves on.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
fun AndroidNowPlayingBarContainer(session: OpenLibrary) {
    val player = session.playback
    val nowPlaying by player.nowPlaying.collectAsState()
    val valuesFailure by player.readFailures.values.collectAsState()
    val repeatMode by player.repeatMode.collectAsState()

    val failure = valuesFailure
    val track = nowPlaying
    if (failure == null && track == null) return

    var queueOpen by remember { mutableStateOf(false) }
    if (queueOpen) {
        AndroidSheet(onDismiss = { queueOpen = false }) {
            // The queue's title is a heading over its lanes, not a sheet title.
            Text(
                text = stringResource(R.string.queue),
                style = ThemeText.heading.style,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.group),
            )
            QueueScreen(session = session, onDismiss = { queueOpen = false })
        }
    }

    // Tapping the track area, not the transport buttons, opens the full-screen
    // player.
    var expanded by remember { mutableStateOf(false) }
    if (expanded) {
        AndroidFullPlayerContainer(session = session, onDismiss = { expanded = false })
    }
    track?.let { AndroidSidePause(session = session, track = it) }

    AndroidNowPlayingBar(
        track = track,
        failure = failure,
        onExpand = { expanded = true },
        transport = {
            NowPlayingTransportButtons(
                session = session,
                player = player,
                repeatMode = repeatMode,
                onOpenQueue = { queueOpen = true },
            )
        },
        progress = {
            AndroidSeekBarContainer(
                session = session,
                player = player,
                modifier = Modifier.fillMaxWidth(),
            )
        },
        castEnded = { CastEndedLine(session) },
    )
}

/** Why casting ended, when the last device's link was lost, until playback moves on. */
@Composable
private fun CastEndedLine(session: OpenLibrary) {
    val castStatus by session.castStore.status.collectAsState()
    val ended = LocalContext.current.castEndedFailure(castStatus) ?: return
    Text(
        text = ended.line,
        style = ThemeText.detail.style,
        color = StatusTone.WARNING.color,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/** [AndroidSidePause] wired to [session]. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun AndroidSidePause(
    session: OpenLibrary,
    track: fm.bae.app.playback.NowPlaying,
) {
    AndroidSidePause(
        track = track,
        onTurnOffPauseBetweenSides = {
            session.messages.perform("turn off pause-between-sides from the side-pause prompt") {
                withContext(Dispatchers.IO) {
                    session.appHandle.setPauseBetweenSides(false)
                }
            }
        },
        onPlay = { session.playback.play() },
        onClose = { session.appHandle.cancelSidePauseCountdown() },
    )
}

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun NowPlayingTransportButtons(
    session: OpenLibrary,
    player: fm.bae.app.playback.BaeCorePlayer,
    repeatMode: BridgeRepeatMode,
    onOpenQueue: () -> Unit,
) {
    val isPlaying by player.isPlayingOrLoading.collectAsState()
    val waitsOnStorage by player.waitsOnStorage.collectAsState()
    NowPlayingBarControls(
        isPlaying = isPlaying,
        waitsOnStorage = waitsOnStorage,
        repeatMode = repeatMode,
        onPrevious = { player.seekToPreviousMediaItem() },
        onPlayPause = { player.togglePlayPause() },
        onNext = { player.seekToNextMediaItem() },
        castButton = { CastButton(session) },
        onOpenQueue = onOpenQueue,
        // The new mode arrives through the playback subscription; a failure shows as a message.
        onCycleRepeat = { cycleRepeatMode(session, repeatMode) },
    )
}

/** Set the repeat mode that follows [repeatMode]; a failure shows as a session message. */
internal fun cycleRepeatMode(
    session: OpenLibrary,
    repeatMode: BridgeRepeatMode,
) {
    session.messages.perform("setRepeatMode") {
        session.appHandle.setRepeatMode(bridgeNextRepeatMode(repeatMode))
    }
}
