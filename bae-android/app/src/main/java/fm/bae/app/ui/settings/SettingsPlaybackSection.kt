package fm.bae.app.ui.settings

import android.content.Context
import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.RestorePlaybackPref
import fm.bae.app.currentLocale
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeSidePauseCountdown

private val logger = BaeLogger("bae.SettingsPlaybackSection")

@Composable
internal fun SettingsPlaybackSection(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = stringResource(R.string.settings_playback),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        PauseBetweenSidesRow(session = session, config = config, ioDispatcher = ioDispatcher)
        SidePauseCountdownRow(session = session, config = config, ioDispatcher = ioDispatcher)
        RestoreOnLaunchRow()
        Text(
            text = stringResource(R.string.settings_restore_on_launch_help),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun PauseBetweenSidesRow(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = stringResource(R.string.settings_pause_between_sides),
            modifier = Modifier.weight(1f),
        )
        Switch(
            checked = config.pauseBetweenSides,
            onCheckedChange = { enabled ->
                scope.launch {
                    performBridgeAction(
                        logger = logger,
                        operation = "update pause-between-sides setting",
                        errors = LocaleErrorLines(context),
                        showError = session.configStore::showError,
                    ) {
                        withContext(ioDispatcher) {
                            session.appHandle.setPauseBetweenSides(enabled)
                        }
                    }
                }
            },
        )
    }
}

@Composable
private fun SidePauseCountdownRow(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    SidePauseCountdownPicker(
        pauseBetweenSides = config.pauseBetweenSides,
        selected = config.sidePauseCountdown,
        onSelect = { countdown ->
            scope.launch {
                performBridgeAction(
                    logger = logger,
                    operation = "update side-pause countdown setting",
                    errors = LocaleErrorLines(context),
                    showError = session.configStore::showError,
                ) {
                    withContext(ioDispatcher) {
                        session.appHandle.setSidePauseCountdown(countdown)
                    }
                }
            }
        },
    )
}

/**
 * Whether a side or disc pause ends on its own, and after how long. Drawn
 * directly under the pause-between-sides switch, and only while that switch
 * is on — with pausing off there is no pause to count down.
 */
@Composable
internal fun SidePauseCountdownPicker(
    pauseBetweenSides: Boolean,
    selected: BridgeSidePauseCountdown,
    onSelect: (BridgeSidePauseCountdown) -> Unit,
) {
    if (!pauseBetweenSides) return
    val context = LocalContext.current
    var expanded by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = stringResource(R.string.settings_side_pause_countdown),
            modifier = Modifier.weight(1f),
        )
        Box {
            TextButton(onClick = { expanded = true }) {
                Text(context.sidePauseCountdownLabel(selected))
            }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                BridgeSidePauseCountdown.entries.forEach { choice ->
                    DropdownMenuItem(
                        text = { Text(context.sidePauseCountdownLabel(choice)) },
                        onClick = {
                            expanded = false
                            onSelect(choice)
                        },
                    )
                }
            }
        }
    }
}

/** How long a countdown choice lasts, in seconds, or null for Off. */
internal val BridgeSidePauseCountdown.seconds: Int?
    get() =
        when (this) {
            BridgeSidePauseCountdown.OFF -> null
            BridgeSidePauseCountdown.SECONDS5 -> 5
            BridgeSidePauseCountdown.SECONDS15 -> 15
            BridgeSidePauseCountdown.SECONDS30 -> 30
            BridgeSidePauseCountdown.SECONDS45 -> 45
            BridgeSidePauseCountdown.SECONDS60 -> 60
        }

/**
 * A countdown choice in words for the current locale: "Off", or a length
 * ("5 seconds") from the platform's measure formatter, so every locale gets its
 * own plural.
 */
internal fun Context.sidePauseCountdownLabel(countdown: BridgeSidePauseCountdown): String {
    val seconds = countdown.seconds ?: return getString(R.string.settings_side_pause_countdown_off)
    return MeasureFormat
        .getInstance(currentLocale(), MeasureFormat.FormatWidth.WIDE)
        .format(Measure(seconds, MeasureUnit.SECOND))
}

// Device-local, not library config: whether the next launch restores the last
// session's playback. The core keeps the resume row current either way, so
// flipping this on takes effect at the next launch.
@Composable
private fun RestoreOnLaunchRow() {
    val context = LocalContext.current
    var restoreOnLaunch by remember { mutableStateOf(RestorePlaybackPref.load(context)) }
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = stringResource(R.string.settings_restore_on_launch),
            modifier = Modifier.weight(1f),
        )
        Switch(
            checked = restoreOnLaunch,
            onCheckedChange = { enabled ->
                restoreOnLaunch = enabled
                RestorePlaybackPref.save(context, enabled)
            },
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun RestoreOnLaunchRowPreview() {
    BaeTheme {
        RestoreOnLaunchRow()
    }
}
