package fm.bae.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.QueueMusic
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.RepeatOne
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.media3.common.C
import fm.bae.app.OpenLibrary
import uniffi.bae_bridge.BridgeRepeatMode

/**
 * Persistent now-playing bar. Reads transport state from the session's
 * [fm.bae.app.playback.BaeCorePlayer] (a pure projection of bae-core's
 * playback), sends transport commands through the same player, and opens the
 * [QueueScreen] in a bottom sheet for queue management. Hidden until something
 * is loaded.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NowPlayingBar(session: OpenLibrary) {
    val player = session.playback
    val nowPlaying by player.nowPlaying.collectAsState()
    val isPlaying by player.isPlaying.collectAsState()
    val isLoading by player.isLoading.collectAsState()
    val position by player.position.collectAsState()
    val repeatMode by player.repeatMode.collectAsState()

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

    // Tapping the track area (cover + title/artist, not the transport buttons)
    // expands to the full-screen player.
    var expanded by remember { mutableStateOf(false) }
    if (expanded) {
        ExpandedNowPlayingScreen(session = session, onDismiss = { expanded = false })
    }

    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 3.dp) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                // The cover + title/artist region is one tap target that expands
                // the full-screen player; the transport buttons below stay outside
                // it so their taps don't expand.
                Row(
                    modifier = Modifier
                        .weight(1f)
                        .clickable { expanded = true }
                        // Announce the whole region as one TalkBack element named
                        // for the track (the cover stays decorative; its info is
                        // in the title/artist text), instead of an unnamed button
                        // plus loose text fragments.
                        .semantics(mergeDescendants = true) {
                            contentDescription =
                                "Now playing, ${track.title} by ${track.artist}"
                        },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CoverImage(
                        path = track.coverPath,
                        cornerRadius = 4.dp,
                        iconPadding = 12.dp,
                        modifier = Modifier.size(48.dp),
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                text = track.title,
                                style = MaterialTheme.typography.bodyMedium,
                                fontWeight = FontWeight.Medium,
                                maxLines = 1,
                                modifier = Modifier.weight(1f, fill = false),
                            )
                        }
                        Text(
                            text = track.artist,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                        )
                    }
                }
                IconButton(onClick = { player.seekToPreviousMediaItem() }) {
                    Icon(Icons.Filled.SkipPrevious, contentDescription = "Previous track")
                }
                PlayPauseControl(
                    isPlaying = isPlaying,
                    isLoading = isLoading,
                    iconSize = 24.dp,
                    spinnerSize = 24.dp,
                    spinnerStroke = 2.dp,
                    onToggle = { player.togglePlayPause() },
                )
                IconButton(onClick = { player.seekToNextMediaItem() }) {
                    Icon(Icons.Filled.SkipNext, contentDescription = "Next track")
                }
                IconButton(onClick = { queueOpen = true }) {
                    Icon(Icons.AutoMirrored.Filled.QueueMusic, contentDescription = "Queue")
                }
                // cycle_repeat_mode is non-throwing; core emits RepeatModeChanged
                // which updates the repeatMode flow above. NONE is dimmed; ALBUM
                // and TRACK are accented (TRACK uses the repeat-one glyph).
                IconButton(onClick = { session.appHandle.cycleRepeatMode() }) {
                    Icon(
                        imageVector = if (repeatMode == BridgeRepeatMode.TRACK) {
                            Icons.Filled.RepeatOne
                        } else {
                            Icons.Filled.Repeat
                        },
                        contentDescription = "Repeat mode",
                        tint = if (repeatMode == BridgeRepeatMode.NONE) {
                            MaterialTheme.colorScheme.onSurfaceVariant
                        } else {
                            MaterialTheme.colorScheme.primary
                        },
                    )
                }
            }

            // While dragging, follow the finger; the progress events would
            // otherwise snap the thumb back mid-drag. Null means "not dragging".
            var dragRatio by remember { mutableStateOf<Float?>(null) }
            val shownRatio = dragRatio ?: position.progress.toFloat().coerceIn(0f, 1f)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = position.elapsedLabel,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Slider(
                    value = shownRatio,
                    onValueChange = { dragRatio = it },
                    onValueChangeFinished = {
                        dragRatio?.let { ratio ->
                            val duration = player.duration
                            if (duration != C.TIME_UNSET && duration > 0L) {
                                player.seekTo((ratio * duration).toLong())
                            }
                        }
                        dragRatio = null
                    },
                    modifier = Modifier.weight(1f).padding(horizontal = 8.dp),
                )
                Text(
                    text = position.remainingLabel,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}
