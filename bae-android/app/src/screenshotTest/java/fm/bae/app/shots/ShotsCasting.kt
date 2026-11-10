package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.playback.CastReceiverDetailsRows
import fm.bae.app.ui.playback.CastReceiverMenuItems
import fm.bae.app.ui.playback.CastReceiverPlace
import fm.bae.app.ui.playback.CastReceiverRole
import fm.bae.app.ui.playback.CastReceiverRows
import fm.bae.app.ui.settings.CastingPage
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeRendererKind

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastingSettingsFound() {
    CastingSettingsScene(receivers = castReceivers())
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastPicker() {
    CastPickerScene(castReceivers())
}

/** The cast picker over receivers whose names are longer than a row: two lines, then an ellipsis. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastPickerLongNames() {
    CastPickerScene(
        castReceivers(
            casting = "Upstairs Living Room Sonos Arc Soundbar",
            dlna = "Denon AVR-X3800H Home Theater Receiver (Basement)",
        ),
    )
}

@Composable
private fun CastPickerScene(receivers: BridgeCastReceiverList) {
    TitledSheetShot(stringResource(R.string.cast)) {
        CastReceiverRows(
            place = CastReceiverPlace.PICKER,
            receivers = receivers,
            connectingName = null,
            onCast = {},
            onDisconnect = {},
            looking = stringResource(R.string.cast_looking_for_devices),
        )
    }
}

/** Living Room cast to, as core lists it, then the rest by name. */
private fun castReceivers(
    casting: String = "Living Room",
    dlna: String = "Den receiver",
) = BridgeCastReceiverList(
    casting =
        BridgeCastReceiver(
            id = "cast-living-room",
            name = casting,
            kind = BridgeRendererKind.CAST,
            model = "Google Nest Audio",
            address = "192.168.1.42",
        ),
    found =
        listOf(
            BridgeCastReceiver(
                id = "cast-den",
                name = dlna,
                kind = BridgeRendererKind.DLNA,
                model = null,
                address = "192.168.1.57",
            ),
            BridgeCastReceiver(
                id = "cast-kitchen",
                name = "Kitchen",
                kind = BridgeRendererKind.AIR_PLAY,
                model = "HomePod mini",
                address = "192.168.1.63",
            ),
        ),
)

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastingSettingsLooking() {
    CastingSettingsScene(receivers = BridgeCastReceiverList(casting = null, found = emptyList()))
}

/** The ⋮ menu of the receiver cast to, open over the page: Details, then Disconnect. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastingSettingsMenu() {
    Box {
        CastingSettingsScene(receivers = castReceivers())
        BaeTheme {
            Surface(
                shape = MenuDefaults.shape,
                color = MenuDefaults.containerColor,
                tonalElevation = MenuDefaults.TonalElevation,
                shadowElevation = MenuDefaults.ShadowElevation,
                modifier = Modifier.align(Alignment.TopEnd).padding(top = 272.dp, end = 16.dp),
            ) {
                Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(IntrinsicSize.Max)) {
                    CastReceiverMenuItems(
                        role = CastReceiverRole.Casting(onDisconnect = {}),
                        onDetails = {},
                        close = {},
                    )
                }
            }
        }
    }
}

/** The Details sheet of the receiver cast to. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastingReceiverDetails() {
    val casting = checkNotNull(castReceivers().casting)
    TitledSheetShot(casting.name) { CastReceiverDetailsRows(casting) }
}

/** The Casting page with casting on and [receivers] found. */
@Composable
private fun CastingSettingsScene(receivers: BridgeCastReceiverList) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            CastingPage(
                enabled = true,
                receivers = receivers,
                connectingName = null,
                transcodeFormat = BridgeCastTranscodeFormat.MP3,
                onBack = {},
                onCast = {},
                onDisconnect = {},
                onEnabledChange = {},
                onTranscodeFormatChange = {},
            )
        }
    }
}
