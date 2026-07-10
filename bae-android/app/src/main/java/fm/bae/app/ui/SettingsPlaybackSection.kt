package fm.bae.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
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
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.RestorePlaybackPref
import fm.bae.app.localizedLine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeException

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
                    try {
                        withContext(ioDispatcher) {
                            session.appHandle.setPauseBetweenSides(enabled)
                        }
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: BridgeException) {
                        logger.error("Failed to update pause-between-sides setting", e)
                        session.configStore.showError(context.localizedLine(e))
                    } catch (e: Exception) {
                        logger.error("Failed to update pause-between-sides setting", e)
                        session.configStore.showError(e.toString())
                    }
                }
            },
        )
    }
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
