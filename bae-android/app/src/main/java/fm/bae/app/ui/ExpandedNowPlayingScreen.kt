package fm.bae.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.QueueMusic
import androidx.compose.material.icons.automirrored.filled.VolumeOff
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.RepeatOne
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material.icons.filled.VolumeUp
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.media3.common.C
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import uniffi.bae_bridge.BridgeRepeatMode

/**
 * Full-screen now-playing player, presented as a full-bleed [Dialog] from the
 * compact [NowPlayingBar] when its track area is tapped. Renders the same single
 * source as the bar — the session's [fm.bae.app.playback.BaeCorePlayer] (a pure
 * projection of bae-core's playback) — and sends transport/seek through that
 * player and volume/mute/repeat through the bridge (all non-throwing).
 *
 * Pure iterate-and-render: the seek labels and the software-decode flag are
 * pre-derived by core; this screen formats nothing.
 */
@OptIn(ExperimentalMaterial3Api::class)
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
fun ExpandedNowPlayingScreen(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val player = session.playback
    val nowPlaying by player.nowPlaying.collectAsState()
    val position by player.position.collectAsState()
    val repeatMode by player.repeatMode.collectAsState()
    val volume by player.volume.collectAsState()
    val isMuted by player.isMuted.collectAsState()

    val track = nowPlaying ?: return

    var queueOpen by remember { mutableStateOf(false) }
    val sheetState = rememberModalBottomSheetState()
    if (queueOpen) {
        ModalBottomSheet(
            onDismissRequest = { queueOpen = false },
            sheetState = sheetState,
        ) {
            QueueScreen(session = session, onDismiss = { queueOpen = false })
        }
    }

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
            Column(
                modifier =
                    Modifier
                        .fillMaxSize()
                        .safeDrawingPadding()
                        .padding(horizontal = 24.dp, vertical = 16.dp),
            ) {
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Filled.ExpandMore, contentDescription = stringResource(R.string.collapse))
                }

                Spacer(modifier = Modifier.height(16.dp))

                CoverImage(
                    path = track.coverPath,
                    cornerRadius = 8.dp,
                    iconPadding = 64.dp,
                    modifier = Modifier.fillMaxWidth().aspectRatio(1f),
                )

                Spacer(modifier = Modifier.height(24.dp))

                ExpandedTrackInfo(title = track.title, artist = track.artist)

                Spacer(modifier = Modifier.height(24.dp))

                ExpandedSeekSection(position = position, player = player)

                Spacer(modifier = Modifier.height(16.dp))

                ExpandedTransportRow(player = player)

                Spacer(modifier = Modifier.height(16.dp))

                ExpandedSecondaryControls(
                    session = session,
                    repeatMode = repeatMode,
                    onOpenQueue = { queueOpen = true },
                )

                Spacer(modifier = Modifier.height(8.dp))

                ExpandedVolumeRow(session = session, isMuted = isMuted, volume = volume)
            }
        }
    }
}

@Composable
private fun ExpandedTrackInfo(
    title: String,
    artist: String,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
            modifier = Modifier.weight(1f, fill = false),
        )
    }
    Text(
        text = artist,
        style = MaterialTheme.typography.titleMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
    )
}

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun ExpandedSeekSection(
    position: fm.bae.app.playback.PlaybackPosition,
    player: fm.bae.app.playback.BaeCorePlayer,
) {
    // While dragging, follow the finger; the progress events would otherwise snap
    // the thumb back mid-drag. Null means "not dragging". Same pattern as the bar.
    var dragRatio by remember { mutableStateOf<Float?>(null) }
    val shownRatio = dragRatio ?: position.progress.toFloat().coerceIn(0f, 1f)
    Slider(
        value = shownRatio,
        onValueChange = { dragRatio = it },
        onValueChangeFinished = {
            dragRatio?.let { ratio ->
                val duration = player.duration
                if (duration != androidx.media3.common.C.TIME_UNSET && duration > 0L) {
                    player.seekTo((ratio * duration).toLong())
                }
            }
            dragRatio = null
        },
        modifier = Modifier.fillMaxWidth(),
    )
    Row(modifier = Modifier.fillMaxWidth()) {
        Text(
            text = position.elapsedLabel,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(modifier = Modifier.weight(1f))
        Text(
            text = position.remainingLabel,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun ExpandedTransportRow(player: fm.bae.app.playback.BaeCorePlayer) {
    val isPlaying by player.isPlaying.collectAsState()
    val isLoading by player.isLoading.collectAsState()
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = { player.seekToPreviousMediaItem() }) {
            Icon(
                Icons.Filled.SkipPrevious,
                contentDescription = stringResource(R.string.previous_track),
                modifier = Modifier.size(36.dp),
            )
        }
        Spacer(modifier = Modifier.width(24.dp))
        PlayPauseControl(
            isPlaying = isPlaying,
            isLoading = isLoading,
            sizes = PlayPauseControlSizes(iconSize = 48.dp, spinnerSize = 36.dp, spinnerStroke = 3.dp),
            onToggle = { player.togglePlayPause() },
        )
        Spacer(modifier = Modifier.width(24.dp))
        IconButton(onClick = { player.seekToNextMediaItem() }) {
            Icon(
                Icons.Filled.SkipNext,
                contentDescription = stringResource(R.string.next_track),
                modifier = Modifier.size(36.dp),
            )
        }
    }
}

@Composable
private fun ExpandedSecondaryControls(
    session: OpenLibrary,
    repeatMode: BridgeRepeatMode,
    onOpenQueue: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // cycle_repeat_mode is non-throwing; core emits RepeatModeChanged which
        // updates the repeatMode flow. NONE is dimmed; ALBUM and TRACK are accented
        // (TRACK uses the repeat-one glyph). Same logic as the compact bar.
        IconButton(onClick = { session.appHandle.cycleRepeatMode() }) {
            Icon(
                imageVector = if (repeatMode == BridgeRepeatMode.TRACK) Icons.Filled.RepeatOne else Icons.Filled.Repeat,
                contentDescription = stringResource(R.string.repeat_mode),
                tint =
                    if (repeatMode == BridgeRepeatMode.NONE) {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    } else {
                        MaterialTheme.colorScheme.primary
                    },
            )
        }
        Spacer(modifier = Modifier.width(24.dp))
        IconButton(onClick = onOpenQueue) {
            Icon(Icons.AutoMirrored.Filled.QueueMusic, contentDescription = stringResource(R.string.queue))
        }
    }
}

@Composable
private fun ExpandedVolumeRow(
    session: OpenLibrary,
    isMuted: Boolean,
    volume: Float,
) {
    // Mute toggle + level slider. While dragging, follow the finger from a local
    // value (cleared on release); the VolumeChanged events would otherwise snap the
    // thumb back mid-drag, same as the seek slider above. Both bridge calls are
    // non-throwing; the resulting VolumeChanged/MuteChanged events drive the flows.
    var dragVolume by remember { mutableStateOf<Float?>(null) }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = { session.appHandle.toggleMute() }) {
            Icon(
                imageVector = if (isMuted) Icons.AutoMirrored.Filled.VolumeOff else Icons.Filled.VolumeUp,
                contentDescription = stringResource(if (isMuted) R.string.unmute else R.string.mute),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Slider(
            value = dragVolume ?: volume,
            onValueChange = {
                dragVolume = it
                session.appHandle.setVolume(it)
            },
            onValueChangeFinished = { dragVolume = null },
            valueRange = 0f..1f,
            modifier = Modifier.weight(1f).padding(horizontal = 8.dp),
        )
    }
}
