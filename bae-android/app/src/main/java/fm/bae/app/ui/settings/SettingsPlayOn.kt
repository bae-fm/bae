package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.LifecycleStartEffect
import fm.bae.app.R
import fm.bae.app.data.Cast
import fm.bae.app.data.CastStart
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidMenu
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.RowSwitch
import fm.bae.app.ui.playback.CastReceiverPlace
import fm.bae.app.ui.playback.CastReceiverRows
import uniffi.bae_bridge.BridgeBrowsingFor
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeCastTranscodeFormat

/** What flipping the Play on other devices switch should do. */
sealed interface CastToggleAction {
    /** Write the setting straight through. */
    data class Apply(
        val enabled: Boolean,
    ) : CastToggleAction

    /** Turning it off would end playback on this receiver — ask first. */
    data class ConfirmDisconnect(
        val device: String,
    ) : CastToggleAction
}

/** Turning Play on other devices off while a receiver plays ends it, so that one case asks first. */
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
 * The Play on group of the Playback page: the switch Play on other devices, which core enforces, and, while it is on,
 * the receivers bae finds and what a track a receiver can't play directly is converted to. The config subscription
 * moves the switch, so a refused flip leaves it in place. [receivers] and [connecting] read their state here, so a
 * receiver found recomposes this group and not the page around it.
 */
@Composable
internal fun SettingsPlayOn(
    enabled: Boolean,
    receivers: () -> BridgeCastReceiverList,
    connecting: () -> CastStart.Connecting?,
    transcodeFormat: BridgeCastTranscodeFormat,
    onEnabledChange: (Boolean) -> Unit,
    onCast: (BridgeCastReceiver) -> Unit,
    onDisconnect: () -> Unit,
    onTranscodeFormatChange: (BridgeCastTranscodeFormat) -> Unit,
) {
    AndroidCard {
        AndroidRow(
            title = stringResource(R.string.settings_play_on_other_devices),
            switch = RowSwitch(enabled, onEnabledChange),
        )
    }
    AndroidNote(stringResource(R.string.settings_play_on_help))
    if (enabled) {
        AndroidSection(text = stringResource(R.string.settings_receivers))
        AndroidCard {
            CastReceiverRows(
                place = CastReceiverPlace.PAGE,
                receivers = receivers(),
                connecting = connecting(),
                onCast = onCast,
                onDisconnect = onDisconnect,
                looking = stringResource(R.string.settings_receivers_looking),
            )
        }
        AndroidCard {
            CastTranscodeFormatPicker(selected = transcodeFormat, onSelect = onTranscodeFormatChange)
        }
        AndroidNote(stringResource(R.string.settings_cast_transcode_help))
    }
}

/** Browses for receivers while the page is on screen with Play on other devices on, and never in the background. */
@Composable
internal fun PlayOnDiscovery(
    cast: Cast,
    enabled: Boolean,
) {
    if (enabled) {
        // A page can sit open with the app in the background, where a browse would hold the multicast lock for nothing.
        LifecycleStartEffect(cast) {
            cast.startDiscovery(BridgeBrowsingFor.SETTINGS_PAGE)
            onStopOrDispose { cast.stopDiscovery(BridgeBrowsingFor.SETTINGS_PAGE) }
        }
    }
}

/** What a track a receiver can't play directly is converted to; core applies it per device flavor. */
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
        AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
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

/** The question turning Play on other devices off raises while a receiver plays, with no line under it. */
@Composable
internal fun PlayOnStopDialog(
    device: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = stringResource(R.string.cast_stop_title, device),
        action = DialogAction(stringResource(R.string.cast_turn_off_confirm), onConfirm),
        dismiss = DialogAction(stringResource(R.string.cancel), onDismiss),
    )
}
