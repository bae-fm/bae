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
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.RepeatOne
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.receiverWithoutVolume
import fm.bae.app.performBridgeAction
import fm.bae.app.playback.artist
import fm.bae.app.playback.library
import fm.bae.app.playback.title
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.Artwork
import fm.bae.app.ui.components.IconTone
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeRepeatMode

private val logger = BaeLogger("bae.AndroidFullPlayerContainer")

/**
 * Full-screen player in a [ModalBottomSheet] opened from [AndroidNowPlayingBarContainer], with
 * the upcoming queue below it in the same scroll.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AndroidFullPlayerContainer(
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
        scrimColor = BaeTheme.colors.scrim,
    ) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxWidth(),
            // The sheet's own inset, as a sheet body gives it, so the player and the queue rows under
            // it start where the design draws them.
            contentPadding =
                PaddingValues(
                    start = ThemeSpace.related,
                    end = ThemeSpace.related,
                    bottom = bottomInset + ThemeSpace.edge,
                ),
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
    val waitsOnStorage by player.waitsOnStorage.collectAsState()
    val repeatMode by player.repeatMode.collectAsState()
    val volume by player.volume.collectAsState()
    val isMuted by player.isMuted.collectAsState()
    val castStatus by session.castStore.status.collectAsState()
    AndroidFullPlayer(
        track = track,
        failure = failure,
        onCollapse = onCollapse,
        seek = { ExpandedSeekSection(session = session, player = player) },
        transport = {
            AndroidTransport(
                isPlaying = isPlaying,
                waitsOnStorage = waitsOnStorage,
                onPrevious = { player.seekToPreviousMediaItem() },
                onPlayPause = { player.togglePlayPause() },
                onNext = { player.seekToNextMediaItem() },
                full = true,
            )
        },
        secondaryControls = {
            ExpandedSecondaryControls(
                repeatMode = repeatMode,
                onCycleRepeat = { cycleRepeatMode(session, repeatMode) },
            )
        },
        volumeRow = {
            AndroidVolumeRow(
                volume = volume,
                isMuted = isMuted,
                onSetMuted = { session.appHandle.setMuted(it) },
                onDragVolume = { session.appHandle.setVolumeLive(it) },
                onSetVolume = { level ->
                    session.messages.performAndWait("setVolume") { session.appHandle.setVolume(level) }
                },
                receiverWithoutVolume = receiverWithoutVolume(castStatus),
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
internal fun AndroidFullPlayer(
    track: fm.bae.app.playback.NowPlaying,
    failure: DisplayError?,
    onCollapse: () -> Unit,
    seek: @Composable () -> Unit,
    transport: @Composable () -> Unit,
    secondaryControls: @Composable () -> Unit,
    volumeRow: @Composable () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.section, vertical = ThemeSpace.related)) {
        // Pulled left so the glyph, not the button, sits at the cover's edge.
        AndroidIconButton(
            Icons.Filled.ExpandMore,
            stringResource(R.string.collapse),
            onClick = onCollapse,
            modifier = Modifier.offset(x = -ThemeSpace.group),
        )

        Spacer(modifier = Modifier.height(ThemeSpace.related))

        if (failure != null) {
            AndroidNotice(failure)
        } else {
            Artwork(
                cover = track.display.library?.coverImage,
                cornerRadius = ThemeRadius.cover,
                iconPadding = 64.dp,
                modifier = Modifier.fillMaxWidth().aspectRatio(1f),
            )

            Spacer(modifier = Modifier.height(ThemeSpace.section))

            val context = LocalContext.current
            ExpandedTrackInfo(title = track.display.title(context), artist = track.display.artist(context))
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
    /** Null for a track the library no longer holds. */
    artist: String?,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            text = title,
            style = ThemeText.hero.style,
            maxLines = 1,
            modifier = Modifier.weight(1f, fill = false),
        )
    }
    if (artist != null) {
        Text(
            text = artist,
            style = ThemeText.heading.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
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
    AndroidSeekBarContainer(
        session = session,
        player = player,
        modifier = Modifier.fillMaxWidth(),
        look = SeekLineLook.LARGE,
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
internal fun ExpandedSecondaryControls(
    repeatMode: BridgeRepeatMode,
    onCycleRepeat: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AndroidIconButton(
            icon = if (repeatMode == BridgeRepeatMode.TRACK) Icons.Filled.RepeatOne else Icons.Filled.Repeat,
            contentDescription = stringResource(R.string.repeat_mode),
            onClick = onCycleRepeat,
            tone = if (repeatMode == BridgeRepeatMode.OFF) IconTone.MUTED else IconTone.ACCENT,
        )
    }
}
