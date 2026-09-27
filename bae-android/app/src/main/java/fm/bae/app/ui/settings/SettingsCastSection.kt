package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
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
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.castingDeviceName
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Eyebrow
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeConfig

private val logger = BaeLogger("bae.SettingsCastSection")

/** What flipping the casting toggle should do. */
sealed interface CastToggleAction {
    /** Write the setting straight through. */
    data class Apply(
        val enabled: Boolean,
    ) : CastToggleAction

    /** Turning casting off would end the session on this device — ask first. */
    data class ConfirmDisconnect(
        val device: String,
    ) : CastToggleAction
}

/** Turning casting off mid-session ends it, so that one case asks first. */
fun castToggleAction(
    enabled: Boolean,
    castingDeviceName: String?,
): CastToggleAction =
    if (!enabled && castingDeviceName != null) {
        CastToggleAction.ConfirmDisconnect(castingDeviceName)
    } else {
        CastToggleAction.Apply(enabled)
    }

/** The Casting settings section: one toggle for the whole feature, which core enforces. */
@Composable
internal fun SettingsCastSection(
    session: OpenLibrary,
    config: BridgeConfig,
    ioDispatcher: CoroutineDispatcher,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val status by session.castStore.status.collectAsState()
    var pendingDisconnect by remember { mutableStateOf<String?>(null) }

    fun setEnabled(enabled: Boolean) {
        scope.launch {
            performBridgeAction(
                logger = logger,
                operation = "update casting setting",
                errors = LocaleErrorLines(context),
                showError = session.configStore::showError,
            ) {
                withContext(ioDispatcher) { session.cast.setEnabled(enabled) }
            }
        }
    }

    pendingDisconnect?.let { device ->
        CastDisconnectDialog(
            device = device,
            onConfirm = {
                pendingDisconnect = null
                setEnabled(false)
            },
            onDismiss = { pendingDisconnect = null },
        )
    }

    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Eyebrow(text = stringResource(R.string.settings_casting))
        CastEnabledRow(
            enabled = config.castEnabled,
            castingDeviceName = castingDeviceName(status),
            onApply = ::setEnabled,
            onConfirmDisconnect = { pendingDisconnect = it },
        )
        Text(
            text = stringResource(R.string.settings_casting_help),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** The casting toggle; the config subscription moves the switch, so a refused flip leaves it in place. */
@Composable
private fun CastEnabledRow(
    enabled: Boolean,
    castingDeviceName: String?,
    onApply: (Boolean) -> Unit,
    onConfirmDisconnect: (String) -> Unit,
) {
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            text = stringResource(R.string.settings_enable_casting),
            modifier = Modifier.weight(1f),
        )
        Switch(
            checked = enabled,
            onCheckedChange = {
                when (val action = castToggleAction(it, castingDeviceName)) {
                    is CastToggleAction.Apply -> onApply(action.enabled)
                    is CastToggleAction.ConfirmDisconnect -> onConfirmDisconnect(action.device)
                }
            },
        )
    }
}

@Composable
private fun CastDisconnectDialog(
    device: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.cast_turn_off_title)) },
        text = { Text(stringResource(R.string.cast_turn_off_message, device)) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(stringResource(R.string.cast_turn_off_confirm))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}
