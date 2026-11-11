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
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.ExpandMore
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
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.rememberScreenScope
import fm.bae.fastvalues.LatestSource
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.bridgeNextRepeatMode

private val logger = BaeLogger("bae.ExpandedNowPlayingScreen")

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
    val valuesFailure by player.readFailures.values.collectAsState()
    val track = nowPlaying ?: return

    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val listState = rememberLazyListState()
    val queue = rememberReorderableQueue(session, listState)
    // Pad past the navigation bar so the last queue row clears it.
    val bottomInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    // Lets the chevron animate the sheet out before dismissing.
    val scope = rememberScreenScope()

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
                    failure = valuesFailure,
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
                queue = queue,
                hasNowPlaying = true,
                onSkipped = null,
            )
        }
    }
}

/** The player wired to [session]. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun ExpandedPlayer(
    session: OpenLibrary,
    track: fm.bae.app.playback.NowPlaying,
    failure: DisplayError?,
    onCollapse: () -> Unit,
) {
    val player = session.playback
    val isPlaying by player.isPlayingOrLoading.collectAsState()
    val loadingTrackId by player.loadingTrackId.collectAsState()
    val repeatMode by player.repeatMode.collectAsState()
    val volume by player.volume.collectAsState()
    val isMuted by player.isMuted.collectAsState()
    ExpandedPlayerContent(
        track = track,
        failure = failure,
        onCollapse = onCollapse,
        seek = { ExpandedSeekSection(session = session, player = player) },
        transport = {
            ExpandedTransportRow(
                isPlaying = isPlaying,
                storageWait = loadingTrackId?.let { player.trackStorageWaits.source(it) },
                onPrevious = { player.seekToPreviousMediaItem() },
                onPlayPause = { player.togglePlayPause() },
                onNext = { player.seekToNextMediaItem() },
            )
        },
        secondaryControls = {
            ExpandedSecondaryControls(
                repeatMode = repeatMode,
                onCycleRepeat = { session.appHandle.setRepeatMode(bridgeNextRepeatMode(repeatMode)) },
            )
        },
        volumeRow = {
            ExpandedVolumeRow(
                volume = volume,
                isMuted = isMuted,
                onSetMuted = { session.appHandle.setMuted(it) },
                onDragVolume = { session.appHandle.setVolumeLive(it) },
                onSetVolume = { session.appHandle.setVolume(it) },
            )
        },
    )
}

/**
 * The player: the track's cover and names over the [seek] row, the
 * [transport], the [secondaryControls] and the [volumeRow]. A failed read of
 * playback shows in place of the track, which is no longer known to be
 * current.
 */
@Composable
internal fun ExpandedPlayerContent(
    track: fm.bae.app.playback.NowPlaying,
    failure: DisplayError?,
    onCollapse: () -> Unit,
    seek: @Composable () -> Unit,
    transport: @Composable () -> Unit,
    secondaryControls: @Composable () -> Unit,
    volumeRow: @Composable () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.section, vertical = ThemeSpace.related)) {
        IconButton(onClick = onCollapse) {
            Icon(Icons.Filled.ExpandMore, contentDescription = stringResource(R.string.collapse))
        }

        Spacer(modifier = Modifier.height(ThemeSpace.related))

        if (failure != null) {
            AndroidNotice(failure)
        } else {
            CoverImage(
                cover = track.coverImage,
                cornerRadius = ThemeRadius.cover,
                iconPadding = 64.dp,
                modifier = Modifier.fillMaxWidth().aspectRatio(1f),
            )

            Spacer(modifier = Modifier.height(ThemeSpace.section))

            ExpandedTrackInfo(title = track.title, artist = track.artist)
        }

        Spacer(modifier = Modifier.height(ThemeSpace.section))

        seek()

        Spacer(modifier = Modifier.height(ThemeSpace.edge))

        transport()

        Spacer(modifier = Modifier.height(ThemeSpace.edge))

        secondaryControls()

        Spacer(modifier = Modifier.height(ThemeSpace.related))

        volumeRow()
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

/** The seek bar; a failed toggle of its time label shows under it, since this sheet covers the snackbar. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun ExpandedSeekSection(
    session: OpenLibrary,
    player: fm.bae.app.playback.BaeCorePlayer,
) {
    val scope = rememberScreenScope()
    var failure by remember { mutableStateOf<DisplayError?>(null) }
    PlaybackProgressAndroidView(
        session = session,
        player = player,
        modifier = Modifier.fillMaxWidth(),
        perform = { operation, action ->
            failure = null
            scope.launch {
                performBridgeAction(logger, operation, session.errors, { failure = it }, action)
            }
        },
    )
    failure?.let { AndroidNotice(it) }
}

@Composable
internal fun ExpandedTransportRow(
    isPlaying: Boolean,
    storageWait: LatestSource<Boolean>?,
    onPrevious: () -> Unit,
    onPlayPause: () -> Unit,
    onNext: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onPrevious) {
            Icon(
                Icons.Filled.SkipPrevious,
                contentDescription = stringResource(R.string.previous_track),
                modifier = Modifier.size(skipGlyphSize),
            )
        }
        Spacer(modifier = Modifier.width(ThemeSpace.section))
        PlayPauseControl(
            isPlaying = isPlaying,
            iconSize = 48.dp,
            storageWait = storageWait,
            onToggle = onPlayPause,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.section))
        IconButton(onClick = onNext) {
            Icon(
                Icons.Filled.SkipNext,
                contentDescription = stringResource(R.string.next_track),
                modifier = Modifier.size(skipGlyphSize),
            )
        }
    }
}

@Composable
internal fun ExpandedSecondaryControls(
    repeatMode: BridgeRepeatMode,
    onCycleRepeat: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onCycleRepeat) {
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
internal fun ExpandedVolumeRow(
    volume: Float,
    isMuted: Boolean,
    onSetMuted: (Boolean) -> Unit,
    onDragVolume: (Float) -> Unit,
    onSetVolume: (Float) -> Unit,
) {
    // Follow the finger from a local value while dragging, moving the output
    // as it goes; the level the drag ends on is set once, where it is stored,
    // and stays shown until the stored volume changes.
    var dragVolume by remember { mutableStateOf<Float?>(null) }
    LaunchedEffect(volume) { dragVolume = null }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = { onSetMuted(!isMuted) }) {
            Icon(
                imageVector = if (isMuted) Icons.AutoMirrored.Filled.VolumeOff else Icons.AutoMirrored.Filled.VolumeUp,
                contentDescription = stringResource(if (isMuted) R.string.unmute else R.string.mute),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Slider(
            value = dragVolume ?: volume,
            onValueChange = {
                dragVolume = it
                onDragVolume(it)
            },
            onValueChangeFinished = { dragVolume?.let(onSetVolume) },
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
