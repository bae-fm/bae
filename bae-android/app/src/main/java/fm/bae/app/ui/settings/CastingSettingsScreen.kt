package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.LifecycleStartEffect
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.castingDeviceName
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.RowSwitch
import fm.bae.app.ui.playback.CastReceiverPlace
import fm.bae.app.ui.playback.CastReceiverRows
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeBrowsingFor
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeCastTranscodeFormat

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

/**
 * The Casting page: one toggle for the whole feature, which core enforces, and, while casting is on, the receivers
 * bae finds and what a track a device can't play directly is converted to.
 */
@Composable
internal fun CastingSettingsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
    ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    val config by session.configStore.config.collectAsState()
    val status by session.castStore.status.collectAsState()
    val receivers by session.castStore.receivers.collectAsState()
    val connecting by session.castStart.connecting.collectAsState()
    var pendingDisconnect by remember { mutableStateOf<String?>(null) }

    fun setTranscodeFormat(format: BridgeCastTranscodeFormat) {
        session.messages.perform("update cast conversion format") {
            withContext(ioDispatcher) { session.cast.setTranscodeFormat(format) }
        }
    }

    fun setEnabled(enabled: Boolean) {
        session.messages.perform("update casting setting") {
            withContext(ioDispatcher) { session.cast.setEnabled(enabled) }
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

    if (config.castEnabled) {
        // A page can sit open with the app in the background, where a browse would hold the multicast lock for nothing.
        LifecycleStartEffect(session.cast) {
            session.cast.startDiscovery(BridgeBrowsingFor.SETTINGS_PAGE)
            onStopOrDispose { session.cast.stopDiscovery(BridgeBrowsingFor.SETTINGS_PAGE) }
        }
    }

    AndroidSettingsCasting(
        enabled = config.castEnabled,
        receivers = receivers,
        connectingName = connecting?.name,
        transcodeFormat = config.castTranscodeFormat,
        onBack = onBack,
        onCast = { receiver -> session.castStart.cast(receiver) },
        onDisconnect = session.cast::stopCasting,
        onEnabledChange = {
            when (val action = castToggleAction(it, castingDeviceName(status))) {
                is CastToggleAction.Apply -> setEnabled(action.enabled)
                is CastToggleAction.ConfirmDisconnect -> pendingDisconnect = action.device
            }
        },
        onTranscodeFormatChange = ::setTranscodeFormat,
    )
}

/** What the Casting page shows; the config subscription moves the switch, so a refused flip leaves it in place. */
@Composable
internal fun AndroidSettingsCasting(
    enabled: Boolean,
    receivers: BridgeCastReceiverList,
    connectingName: String?,
    transcodeFormat: BridgeCastTranscodeFormat,
    onBack: () -> Unit,
    onCast: (BridgeCastReceiver) -> Unit,
    onDisconnect: () -> Unit,
    onEnabledChange: (Boolean) -> Unit,
    onTranscodeFormatChange: (BridgeCastTranscodeFormat) -> Unit,
) {
    AndroidScreen(
        title = stringResource(R.string.settings_casting),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            AndroidCard {
                CastEnabledRow(enabled = enabled, onChange = onEnabledChange)
            }
            CastingCaption(stringResource(R.string.settings_casting_help))
            if (enabled) {
                AndroidSection(text = stringResource(R.string.settings_casting_receivers))
                AndroidCard {
                    CastReceiverRows(
                        place = CastReceiverPlace.PAGE,
                        receivers = receivers,
                        connectingName = connectingName,
                        onCast = onCast,
                        onDisconnect = onDisconnect,
                        looking = stringResource(R.string.settings_casting_looking),
                    )
                }
                AndroidCard {
                    CastTranscodeFormatPicker(selected = transcodeFormat, onSelect = onTranscodeFormatChange)
                }
                CastingCaption(stringResource(R.string.settings_cast_transcode_help))
            }
        }
    }
}

/** What the card above it does, in line with the card's rows. */
@Composable
private fun CastingCaption(text: String) {
    Text(
        text = text,
        modifier = Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.related),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** What a track a device can't play directly is converted to; core applies it per device flavor. */
@Composable
private fun CastTranscodeFormatPicker(
    selected: BridgeCastTranscodeFormat,
    onSelect: (BridgeCastTranscodeFormat) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidRow(
            title = stringResource(R.string.settings_cast_transcode_format),
            subtitle = castTranscodeFormatLabel(selected),
            onClick = { expanded = true },
        )
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            BridgeCastTranscodeFormat.entries.forEach { choice ->
                DropdownMenuItem(
                    text = { Text(castTranscodeFormatLabel(choice)) },
                    onClick = {
                        expanded = false
                        onSelect(choice)
                    },
                )
            }
        }
    }
}

/** A conversion choice as the picker shows it. "MP3" is the format's name, the same in every language. */
@Composable
private fun castTranscodeFormatLabel(format: BridgeCastTranscodeFormat): String =
    when (format) {
        BridgeCastTranscodeFormat.MP3 -> "MP3"
        BridgeCastTranscodeFormat.WAV -> stringResource(R.string.settings_cast_transcode_wav)
    }

@Composable
private fun CastEnabledRow(
    enabled: Boolean,
    onChange: (Boolean) -> Unit,
) {
    AndroidRow(
        title = stringResource(R.string.settings_enable_casting),
        switch = RowSwitch(enabled, onChange),
    )
}

@Composable
private fun CastDisconnectDialog(
    device: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = stringResource(R.string.cast_turn_off_title),
        message = stringResource(R.string.cast_turn_off_message, device),
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = onConfirm) {
                Text(stringResource(R.string.cast_turn_off_confirm))
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}
