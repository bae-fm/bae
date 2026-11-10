package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.castEndedFailure
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.NeutralTextButton
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.color
import fm.bae.app.ui.countdownSecondsLeft
import fm.bae.app.ui.rememberCountdownNow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeSideCountdown
import uniffi.bae_bridge.bridgeNextRepeatMode

/**
 * Persistent now-playing bar, hidden until something is loaded or playback
 * can't be read. A failed read shows in place of the track, which is no
 * longer known to be current; a cast whose link was lost says so under it
 * until playback moves on.
 */
@OptIn(ExperimentalMaterial3Api::class)
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
fun NowPlayingBar(session: OpenLibrary) {
    val player = session.playback
    val nowPlaying by player.nowPlaying.collectAsState()
    val valuesFailure by player.readFailures.values.collectAsState()
    val repeatMode by player.repeatMode.collectAsState()

    val failure = valuesFailure
    val track = nowPlaying
    if (failure == null && track == null) return

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

    // Tapping the track area, not the transport buttons, opens the full-screen
    // player.
    var expanded by remember { mutableStateOf(false) }
    if (expanded) {
        ExpandedNowPlayingScreen(session = session, onDismiss = { expanded = false })
    }
    track?.let { SidePauseAlert(session = session, track = it) }

    NowPlayingBarStrip(
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
            PlaybackProgressAndroidView(
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

/** [SidePauseAlert] wired to [session]. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun SidePauseAlert(
    session: OpenLibrary,
    track: fm.bae.app.playback.NowPlaying,
) {
    val context = LocalContext.current
    SidePauseAlert(
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

/**
 * The prompt core raises when playback pauses at the end of a side or disc; its
 * box starts checked because the prompt only appears while the setting is on.
 */
@Composable
fun SidePauseAlert(
    track: fm.bae.app.playback.NowPlaying,
    onTurnOffPauseBetweenSides: () -> Unit,
    onPlay: () -> Unit,
    onClose: () -> Unit,
    nowMs: () -> Long = System::currentTimeMillis,
) {
    val context = LocalContext.current
    var dismissedPromptId by remember { mutableStateOf<String?>(null) }
    val wording = track.sidePausePrompt
    if (wording != null && dismissedPromptId != wording.prompt.id) {
        val prompt = wording.prompt
        var keepPausing by remember(prompt.id) { mutableStateOf(true) }
        val answer = { play: Boolean ->
            dismissedPromptId = prompt.id
            if (!keepPausing) onTurnOffPauseBetweenSides()
            if (play) onPlay() else onClose()
        }
        AlertDialog(
            onDismissRequest = { answer(false) },
            title = {
                Text(
                    context.coreString(
                        wording.titleKey,
                        mapOf("label" to prompt.sideLabel),
                    ),
                )
            },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                    Text(context.coreString("core.playback.pause.message"))
                    prompt.countdown?.let { countdown ->
                        SidePauseCountdownLine(
                            messageKey = wording.countdownKey,
                            countdown = countdown,
                            nowMs = nowMs,
                        )
                    }
                    KeepPausingCheckbox(
                        labelKey = wording.keepPausingKey,
                        checked = keepPausing,
                        onCheckedChange = { keepPausing = it },
                    )
                }
            },
            confirmButton = {
                PrimaryButton(onClick = { answer(true) }) {
                    Text(stringResource(R.string.play))
                }
            },
            dismissButton = {
                NeutralTextButton(onClick = { answer(false) }) {
                    Text(stringResource(R.string.close))
                }
            },
        )
    }
}

/** The checkbox that keeps pausing at the kind of boundary that ended. */
@Composable
private fun KeepPausingCheckbox(
    labelKey: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .toggleable(
                    value = checked,
                    role = Role.Checkbox,
                    onValueChange = onCheckedChange,
                ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = checked, onCheckedChange = null)
        Spacer(modifier = Modifier.width(ThemeSpace.related))
        Text(LocalContext.current.coreString(labelKey))
    }
}

/** The countdown line, redrawn each whole second until core's deadline and read out as it changes. */
@Composable
private fun SidePauseCountdownLine(
    messageKey: String,
    countdown: BridgeSideCountdown,
    nowMs: () -> Long,
) {
    val context = LocalContext.current
    val now = rememberCountdownNow(countdown.resumesAtMs, nowMs)
    Text(
        text =
            context.coreString(
                messageKey,
                mapOf("seconds" to countdownSecondsLeft(countdown.resumesAtMs, now)),
            ),
        style = ThemeText.strong.style,
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
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
    val loadingTrackId by player.loadingTrackId.collectAsState()
    NowPlayingTransport(
        isPlaying = isPlaying,
        storageWait = loadingTrackId?.let { player.trackStorageWaits.source(it) },
        repeatMode = repeatMode,
        onPrevious = { player.seekToPreviousMediaItem() },
        onPlayPause = { player.togglePlayPause() },
        onNext = { player.seekToNextMediaItem() },
        castButton = { CastButton(session) },
        onOpenQueue = onOpenQueue,
        // `setRepeatMode` does not throw; the new mode arrives through the
        // playback subscription.
        onCycleRepeat = { session.appHandle.setRepeatMode(bridgeNextRepeatMode(repeatMode)) },
    )
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
            onTurnOffPauseBetweenSides = {},
            onPlay = {},
            onClose = {},
        )
    }
}
