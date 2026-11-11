package fm.bae.app.ui.settings

import android.content.Context
import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.RestorePlaybackPref
import fm.bae.app.currentLocale
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeSidePauseCountdown

@Composable
internal fun SettingsPlaybackSection(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    Column(modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge)) {
        AndroidCard {
            PauseBetweenSidesRow(session = session, config = config, ioDispatcher = ioDispatcher)
            SidePauseCountdownRow(session = session, config = config, ioDispatcher = ioDispatcher)
            RestoreOnLaunchRow()
        }
    }
}

@Composable
private fun PauseBetweenSidesRow(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    AndroidRow(
        title = stringResource(R.string.settings_pause_between_sides),
        trailing = {
            Switch(
                checked = config.pauseBetweenSides,
                onCheckedChange = { enabled ->
                    session.messages.perform("update pause-between-sides setting") {
                        withContext(ioDispatcher) {
                            session.appHandle.setPauseBetweenSides(enabled)
                        }
                    }
                },
            )
        },
    )
}

@Composable
private fun SidePauseCountdownRow(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    SidePauseCountdownPicker(
        pauseBetweenSides = config.pauseBetweenSides,
        selected = config.sidePauseCountdown,
        onSelect = { countdown ->
            session.messages.perform("update side-pause countdown setting") {
                withContext(ioDispatcher) {
                    session.appHandle.setSidePauseCountdown(countdown)
                }
            }
        },
    )
}

/**
 * Whether a side or disc pause ends on its own, and after how long: a row nested under the
 * pause-between-sides switch, its value as supporting text and a tap opening the choices. While
 * pausing is off there is no pause to count down, so the row shows dimmed and does nothing.
 */
@Composable
internal fun SidePauseCountdownPicker(
    pauseBetweenSides: Boolean,
    selected: BridgeSidePauseCountdown,
    onSelect: (BridgeSidePauseCountdown) -> Unit,
) {
    val context = LocalContext.current
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidRow(
            title = stringResource(R.string.settings_resume_automatically),
            subtitle = context.sidePauseCountdownLabel(selected),
            nested = true,
            enabled = pauseBetweenSides,
            onClick = { expanded = true },
        )
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

/** How long a countdown choice lasts, in seconds, or null for Never. */
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

/** A countdown choice in words for the current locale, with the platform's plural for the length. */
internal fun Context.sidePauseCountdownLabel(countdown: BridgeSidePauseCountdown): String {
    val seconds = countdown.seconds ?: return getString(R.string.settings_resume_never)
    val length =
        MeasureFormat
            .getInstance(currentLocale(), MeasureFormat.FormatWidth.WIDE)
            .format(Measure(seconds, MeasureUnit.SECOND))
    return getString(R.string.settings_resume_after, length)
}

// Device-local, not library config: whether the next launch restores the last session's playback.
@Composable
private fun RestoreOnLaunchRow() {
    val context = LocalContext.current
    var restoreOnLaunch by remember { mutableStateOf(RestorePlaybackPref.load(context)) }
    AndroidRow(
        title = stringResource(R.string.settings_restore_on_launch),
        subtitle = stringResource(R.string.settings_restore_on_launch_help),
        trailing = {
            Switch(
                checked = restoreOnLaunch,
                onCheckedChange = { enabled ->
                    restoreOnLaunch = enabled
                    RestorePlaybackPref.save(context, enabled)
                },
            )
        },
    )
}

@Preview(showBackground = true)
@Composable
private fun RestoreOnLaunchRowPreview() {
    BaeTheme {
        RestoreOnLaunchRow()
    }
}
