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
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.bridgeNextRepeatMode

private val skipGlyphSize = 36.dp

/**
 * Full-screen player in a [ModalBottomSheet] opened from [NowPlayingBar], with
 * the upcoming queue below it in the same scroll.
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
    // Pad past the navigation bar so the last queue row clears it.
    val bottomInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    // Lets the chevron animate the sheet out before dismissing.
    val scope = rememberCoroutineScope()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
    ) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(bottom = bottomInset + ThemeSpace.edge),
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
            // The player shows the current track, so the queue lists only what
            // is next; tapping a row skips without closing the sheet.
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
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.section, vertical = ThemeSpace.related)) {
        IconButton(onClick = onCollapse) {
            Icon(Icons.Filled.ExpandMore, contentDescription = stringResource(R.string.collapse))
        }

        Spacer(modifier = Modifier.height(ThemeSpace.related))

        CoverImage(
            cover = track.coverImage,
            cornerRadius = ThemeRadius.cover,
            iconPadding = 64.dp,
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
        )

        Spacer(modifier = Modifier.height(ThemeSpace.section))

        ExpandedTrackInfo(title = track.title, artist = track.artist)

        Spacer(modifier = Modifier.height(ThemeSpace.section))

        ExpandedSeekSection(session = session, player = session.playback)

        Spacer(modifier = Modifier.height(ThemeSpace.edge))

        ExpandedTransportRow(player = session.playback)

        Spacer(modifier = Modifier.height(ThemeSpace.edge))

        ExpandedSecondaryControls(session = session)

        Spacer(modifier = Modifier.height(ThemeSpace.related))

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
            style = ThemeText.hero.style,
            maxLines = 1,
            modifier = Modifier.weight(1f, fill = false),
        )
    }
    Text(
        text = artist,
        style = ThemeText.heading.style,
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
                modifier = Modifier.size(skipGlyphSize),
            )
        }
        Spacer(modifier = Modifier.width(ThemeSpace.section))
        PlayPauseControl(
            isPlaying = isPlaying,
            isLoading = isLoading,
            sizes = PlayPauseControlSizes(iconSize = 48.dp, spinnerSize = 36.dp, spinnerStroke = 3.dp),
            onToggle = { player.togglePlayPause() },
        )
        Spacer(modifier = Modifier.width(ThemeSpace.section))
        IconButton(onClick = { player.seekToNextMediaItem() }) {
            Icon(
                Icons.Filled.SkipNext,
                contentDescription = stringResource(R.string.next_track),
                modifier = Modifier.size(skipGlyphSize),
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
    // Follow the finger from a local value while dragging; incoming volume
    // updates would otherwise snap the thumb back.
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
            modifier = Modifier.weight(1f).padding(horizontal = ThemeSpace.related),
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
