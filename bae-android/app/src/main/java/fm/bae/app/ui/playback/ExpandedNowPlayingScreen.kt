package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
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
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.CoverImage
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.bridgeNextRepeatMode

/**
 * Full-screen now-playing player, presented as a [ModalBottomSheet] (swipe down
 * to dismiss) from the compact [NowPlayingBar] when its track area is tapped. The
 * player fills the first screen and the upcoming queue sits below it in the same
 * scroll, so a swipe up scrolls into the queue and a swipe down from the top
 * dismisses (the sheet and the list coordinate that handoff via nested scroll).
 * Renders the session's [fm.bae.app.playback.BaeCorePlayer] (a pure projection of
 * bae-core's playback) and sends transport/seek through that player and
 * volume/mute/repeat through the bridge (all non-throwing).
 *
 * Pure iterate-and-render: the seek labels and the software-decode flag are
 * pre-derived by core; this screen formats nothing.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ExpandedNowPlayingScreen(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val player = session.playback
    val nowPlaying by player.nowPlaying.collectAsState()
    val track = nowPlaying ?: return

    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val listState = rememberLazyListState()
    val (order, reorderState) = rememberReorderableQueue(session, listState)
    // The sheet handles the top inset; pad the scroll's bottom past the navigation
    // bar so the last queue row clears it.
    val bottomInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    // A swipe dismisses via the sheet's own animation; the chevron animates the
    // sheet out first, then tears it down — onDismiss alone would drop it abruptly.
    val scope = rememberCoroutineScope()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
    ) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(bottom = bottomInset + 16.dp),
        ) {
            item(key = "player") {
                ExpandedPlayer(
                    session = session,
                    track = track,
                    onCollapse = {
                        scope.launch { sheetState.hide() }.invokeOnCompletion {
                            if (!sheetState.isVisible) onDismiss()
                        }
                    },
                )
            }
            // The player above already shows the current track, so the embedded
            // queue is just the up-next list; a row tap skips without closing the
            // player (`onSkipped = null`).
            queueContent(
                session = session,
                order = order,
                reorderState = reorderState,
                hasNowPlaying = true,
                onSkipped = null,
            )
        }
    }
}

@Composable
private fun ExpandedPlayer(
    session: OpenLibrary,
    track: fm.bae.app.playback.NowPlaying,
    onCollapse: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp)) {
        IconButton(onClick = onCollapse) {
            Icon(Icons.Filled.ExpandMore, contentDescription = stringResource(R.string.collapse))
        }

        Spacer(modifier = Modifier.height(8.dp))

        CoverImage(
            cover = track.coverImage,
            loadImage = session.library::imageBytes,
            cornerRadius = 8.dp,
            iconPadding = 64.dp,
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
        )

        Spacer(modifier = Modifier.height(24.dp))

        ExpandedTrackInfo(title = track.title, artist = track.artist)

        Spacer(modifier = Modifier.height(24.dp))

        ExpandedSeekSection(session = session, player = session.playback)

        Spacer(modifier = Modifier.height(16.dp))

        ExpandedTransportRow(player = session.playback)

        Spacer(modifier = Modifier.height(16.dp))

        ExpandedSecondaryControls(session = session)

        Spacer(modifier = Modifier.height(8.dp))

        ExpandedVolumeRow(session = session)
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
    session: OpenLibrary,
    player: fm.bae.app.playback.BaeCorePlayer,
) {
    PlaybackProgressAndroidView(
        session = session,
        player = player,
        modifier = Modifier.fillMaxWidth(),
    )
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
private fun ExpandedSecondaryControls(session: OpenLibrary) {
    val repeatMode by session.playback.repeatMode.collectAsState()
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // set_repeat_mode is non-throwing; core emits RepeatModeChanged which
        // updates the repeatMode flow. OFF is dimmed; CONTEXT and TRACK are accented
        // (TRACK uses the repeat-one glyph). Same logic as the compact bar.
        IconButton(onClick = { session.appHandle.setRepeatMode(bridgeNextRepeatMode(repeatMode)) }) {
            Icon(
                imageVector = if (repeatMode == BridgeRepeatMode.TRACK) Icons.Filled.RepeatOne else Icons.Filled.Repeat,
                contentDescription = stringResource(R.string.repeat_mode),
                tint =
                    if (repeatMode == BridgeRepeatMode.OFF) {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    } else {
                        MaterialTheme.colorScheme.primary
                    },
            )
        }
    }
}

@Composable
private fun ExpandedVolumeRow(session: OpenLibrary) {
    val volume by session.playback.volume.collectAsState()
    val isMuted by session.playback.isMuted.collectAsState()
    // Mute toggle + level slider. While dragging, follow the finger from a local
    // value (cleared on release); the VolumeChanged events would otherwise snap the
    // thumb back mid-drag, same as the seek slider above. Both bridge calls are
    // non-throwing; the resulting VolumeChanged/MuteChanged events drive the flows.
    var dragVolume by remember { mutableStateOf<Float?>(null) }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = { session.appHandle.setMuted(!isMuted) }) {
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

@Preview(showBackground = true)
@Composable
private fun ExpandedTrackInfoPreview() {
    BaeTheme {
        Column {
            ExpandedTrackInfo(title = "Track Title", artist = "Artist Name")
        }
    }
}
