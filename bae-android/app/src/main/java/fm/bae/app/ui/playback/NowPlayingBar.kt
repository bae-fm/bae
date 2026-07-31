package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverBytesCache
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.LocalCoverBytesCache
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.bridgeNextRepeatMode

/**
 * Persistent now-playing bar. Reads transport state from the session's
 * [fm.bae.app.playback.BaeCorePlayer] (a pure projection of bae-core's
 * playback), sends transport commands through the same player, and opens the
 * [QueueScreen] in a bottom sheet for queue management. Hidden until something
 * is loaded.
 */
@OptIn(ExperimentalMaterial3Api::class)
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
fun NowPlayingBar(session: OpenLibrary) {
    val player = session.playback
    val nowPlaying by player.nowPlaying.collectAsState()
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
    SidePauseAlert(track = track)

    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 3.dp) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                NowPlayingTrackInfo(
                    track = track,
                    loadImage = session.library::imageBytes,
                    onExpand = { expanded = true },
                )
                NowPlayingTransportButtons(
                    session = session,
                    player = player,
                    repeatMode = repeatMode,
                    onOpenQueue = { queueOpen = true },
                )
            }
            PlaybackProgressAndroidView(
                session = session,
                player = player,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun RowScope.NowPlayingTrackInfo(
    track: fm.bae.app.playback.NowPlaying,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onExpand: () -> Unit,
) {
    val nowPlayingDescription = stringResource(R.string.now_playing_track_by_artist, track.title, track.artist)
    Row(
        modifier =
            Modifier
                .weight(1f)
                .clickable(onClick = onExpand)
                // Announce the whole region as one TalkBack element named
                // for the track (cover stays decorative; info is in the text),
                // instead of an unnamed button plus loose text fragments.
                .semantics(mergeDescendants = true) { contentDescription = nowPlayingDescription },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = track.coverImage,
            loadImage = loadImage,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                modifier = Modifier.weight(1f, fill = false),
            )
            Text(
                text = track.artist,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
fun SidePauseAlert(track: fm.bae.app.playback.NowPlaying) {
    val context = LocalContext.current
    var dismissedPromptId by remember { mutableStateOf<String?>(null) }
    val prompt = track.sidePausePrompt
    if (prompt != null && dismissedPromptId != prompt.id) {
        AlertDialog(
            onDismissRequest = { dismissedPromptId = prompt.id },
            title = {
                Text(
                    context.coreString(
                        prompt.titleKey,
                        mapOf("letter" to prompt.sideLetter),
                    ),
                )
            },
            text = { Text(context.coreString(prompt.messageKey)) },
            confirmButton = {
                Button(onClick = { dismissedPromptId = prompt.id }) {
                    Text(stringResource(R.string.close))
                }
            },
        )
    }
}

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun NowPlayingTransportButtons(
    session: OpenLibrary,
    player: fm.bae.app.playback.BaeCorePlayer,
    repeatMode: BridgeRepeatMode,
    onOpenQueue: () -> Unit,
) {
    val isPlaying by player.isPlaying.collectAsState()
    val isLoading by player.isLoading.collectAsState()
    IconButton(onClick = { player.seekToPreviousMediaItem() }) {
        Icon(Icons.Filled.SkipPrevious, contentDescription = stringResource(R.string.previous_track))
    }
    PlayPauseControl(
        isPlaying = isPlaying,
        isLoading = isLoading,
        sizes = PlayPauseControlSizes(iconSize = 24.dp, spinnerSize = 24.dp, spinnerStroke = 2.dp),
        onToggle = { player.togglePlayPause() },
    )
    IconButton(onClick = { player.seekToNextMediaItem() }) {
        Icon(Icons.Filled.SkipNext, contentDescription = stringResource(R.string.next_track))
    }
    IconButton(onClick = onOpenQueue) {
        Icon(Icons.AutoMirrored.Filled.QueueMusic, contentDescription = stringResource(R.string.queue))
    }
    // set_repeat_mode is non-throwing; core emits RepeatModeChanged which updates
    // the repeatMode flow. OFF is dimmed; CONTEXT and TRACK are accented (TRACK uses
    // the repeat-one glyph).
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

@Preview(showBackground = true)
@Composable
private fun NowPlayingTrackInfoPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalCoverBytesCache provides CoverBytesCache()) {
            Row {
                NowPlayingTrackInfo(
                    track =
                        NowPlaying(
                            trackId = "trk-1",
                            title = "Track Title",
                            artist = "Artist Name",
                            coverImage = PreviewData.imageRef("rel-1"),
                            sidePausePrompt = null,
                        ),
                    loadImage = { _ -> null },
                    onExpand = {},
                )
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun SidePauseAlertPreview() {
    BaeTheme {
        SidePauseAlert(
            track =
                NowPlaying(
                    trackId = "trk-1",
                    title = "Track Title",
                    artist = "Artist Name",
                    coverImage = PreviewData.imageRef("rel-1"),
                    sidePausePrompt = PreviewData.sidePausePrompt(),
                ),
        )
    }
}
