package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.LocalNowMs
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidCheckbox
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.WIDE_DIALOG_WIDTH
import fm.bae.app.ui.rememberCountdownLine
import uniffi.bae_bridge.BridgeCountdown

/**
 * The prompt core raises when playback pauses at the end of a side or disc; its
 * box starts checked because the prompt only appears while the setting is on.
 */
@Composable
fun AndroidSidePause(
    track: NowPlaying,
    onTurnOffPauseBetweenSides: () -> Unit,
    onPlay: () -> Unit,
    onClose: () -> Unit,
    nowMs: () -> Long = LocalNowMs.current,
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
        AndroidDialog(
            onDismissRequest = { answer(false) },
            parts =
                sidePauseParts(
                    title = wording.title(context),
                    countdown = prompt.countdown,
                    nowMs = nowMs,
                    keepPausingLabel = context.coreString(wording.keepPausingKey),
                    keepPausing = keepPausing,
                    onKeepPausingChange = { keepPausing = it },
                    onPlay = { answer(true) },
                    onClose = { answer(false) },
                ),
        )
    }
}

/**
 * What the side-pause dialog says and offers, from plain values: the [title] naming what ended, the line that
 * playback is paused, the [countdown] to the next start when core has one, the box [keepPausing] with its
 * label, and, once the box is cleared, where to turn the pause back on. Close and Play end it.
 */
@Composable
internal fun sidePauseParts(
    title: String,
    countdown: BridgeCountdown?,
    nowMs: () -> Long,
    keepPausingLabel: String,
    keepPausing: Boolean,
    onKeepPausingChange: (Boolean) -> Unit,
    onPlay: () -> Unit,
    onClose: () -> Unit,
): DialogParts {
    val context = LocalContext.current
    return DialogParts(
        width = WIDE_DIALOG_WIDTH,
        title = title,
        message = context.coreString("core.playback.pause.message"),
        content = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                countdown?.let { SidePauseCountdownLine(countdown = it, nowMs = nowMs) }
                AndroidCheckbox(label = keepPausingLabel, checked = keepPausing, onCheckedChange = onKeepPausingChange)
                // Clearing the box turns the setting off, which also ends these prompts.
                if (!keepPausing) {
                    Text(
                        text = stringResource(R.string.side_pause_turned_off),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        },
        action = DialogAction(stringResource(R.string.play), onPlay, filled = true),
        dismiss = DialogAction(stringResource(R.string.close), onClose),
    )
}

/** The countdown line, redrawn each whole second until core's deadline and read out as it changes. */
@Composable
private fun SidePauseCountdownLine(
    countdown: BridgeCountdown,
    nowMs: () -> Long,
) {
    Text(
        text = rememberCountdownLine(countdown, nowMs),
        style = ThemeText.strong.style,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
    )
}
