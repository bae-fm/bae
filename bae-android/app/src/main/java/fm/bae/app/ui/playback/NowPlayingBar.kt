package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.QueueMusic
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.RepeatOne
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.performBridgeAction
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.PrimaryButton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeSideCountdown
import uniffi.bae_bridge.bridgeNextRepeatMode

private val logger = BaeLogger("bae.NowPlayingBar")

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
    SidePauseAlert(session = session, track = track)

    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 3.dp) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                NowPlayingTrackInfo(
                    track = track,
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

/**
 * [SidePauseAlert] wired to [session]: Play resumes through the player, Close
 * stops core's countdown, and an unchecked box writes the setting off.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun SidePauseAlert(
    session: OpenLibrary,
    track: fm.bae.app.playback.NowPlaying,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    SidePauseAlert(
        track = track,
        onTurnOffPauseBetweenSides = {
            scope.launch {
                performBridgeAction(
                    logger = logger,
                    operation = "turn off pause-between-sides from the side-pause prompt",
                    errors = LocaleErrorLines(context),
                    showError = session.configStore::showError,
                ) {
                    withContext(Dispatchers.IO) {
                        session.appHandle.setPauseBetweenSides(false)
                    }
                }
            }
        },
        onPlay = { session.playback.play() },
        onClose = { session.appHandle.cancelSidePauseCountdown() },
    )
}

/**
 * The prompt core raises when playback pauses at the end of a side or disc. Its
 * checkbox mirrors the "Pause between sides and discs" setting and starts
 * checked — the prompt only appears while the setting is on. Answering it with
 * the box unchecked calls [onTurnOffPauseBetweenSides]; a checked box changes
 * nothing. Play starts the next side now ([onPlay]); Close, or dismissing the
 * dialog, stays paused and stops any countdown ([onClose]), so the next side
 * waits for Play.
 *
 * While core counts down to the next side, the dialog shows the seconds left,
 * read from core's deadline against [nowMs] — the dialog only shows the time;
 * core starts the side.
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
    val prompt = track.sidePausePrompt
    if (prompt != null && dismissedPromptId != prompt.id) {
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
                        prompt.titleKey,
                        mapOf("label" to prompt.sideLabel),
                    ),
                )
            },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(context.coreString("core.playback.pause.message"))
                    prompt.countdown?.let { countdown ->
                        SidePauseCountdownLine(countdown = countdown, nowMs = nowMs)
                    }
                    Row(
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .toggleable(
                                    value = keepPausing,
                                    role = Role.Checkbox,
                                    onValueChange = { keepPausing = it },
                                ),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(checked = keepPausing, onCheckedChange = null)
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(stringResource(R.string.settings_pause_between_sides))
                    }
                }
            },
            confirmButton = {
                PrimaryButton(onClick = { answer(true) }) {
                    Text(stringResource(R.string.play))
                }
            },
            dismissButton = {
                TextButton(onClick = { answer(false) }) {
                    Text(stringResource(R.string.close))
                }
            },
        )
    }
}

/**
 * The line counting down to the next side, redrawn as each whole second before
 * core's deadline runs out. A polite live region, so a screen reader hears the
 * new count without losing its place.
 */
@Composable
private fun SidePauseCountdownLine(
    countdown: BridgeSideCountdown,
    nowMs: () -> Long,
) {
    val context = LocalContext.current
    var now by remember(countdown.resumesAtMs) { mutableLongStateOf(nowMs()) }
    LaunchedEffect(countdown.resumesAtMs) {
        while (now < countdown.resumesAtMs) {
            // Wake as the next whole second before the deadline passes.
            val untilNextSecond = (countdown.resumesAtMs - now) % MILLIS_PER_SECOND
            delay(if (untilNextSecond == 0L) MILLIS_PER_SECOND else untilNextSecond)
            now = nowMs()
        }
    }
    Text(
        text =
            context.coreString(
                countdown.messageKey,
                mapOf("seconds" to sideCountdownSecondsLeft(countdown.resumesAtMs, now)),
            ),
        style = MaterialTheme.typography.bodyLarge,
        fontWeight = FontWeight.Medium,
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
    )
}

/**
 * Whole seconds from [nowMs] until [resumesAtMs], rounded up so the line never
 * reads 0 while the side has yet to start, and never below 0.
 */
internal fun sideCountdownSecondsLeft(
    resumesAtMs: Long,
    nowMs: Long,
): Long = (maxOf(0L, resumesAtMs - nowMs) + MILLIS_PER_SECOND - 1) / MILLIS_PER_SECOND

private const val MILLIS_PER_SECOND = 1_000L

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
    CastButton(session)
    IconButton(onClick = onOpenQueue) {
        Icon(Icons.AutoMirrored.Filled.QueueMusic, contentDescription = stringResource(R.string.queue))
    }
    // set_repeat_mode is non-throwing; the retained playback subscription updates
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
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
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
            onTurnOffPauseBetweenSides = {},
            onPlay = {},
            onClose = {},
        )
    }
}
