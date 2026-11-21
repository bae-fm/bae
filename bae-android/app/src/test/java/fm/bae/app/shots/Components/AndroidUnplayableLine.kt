package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.albumdetail.AndroidUnplayableLine
import org.junit.Test
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg

/** The component shots of `AndroidUnplayableLine`, one for each row of its preview. */
class AndroidUnplayableLineShots : BoardShot() {
    @Test
    fun androidComponentUnplayableLineOnAnotherDevice() =
        component("android.component.unplayable-line.on-another-device") { Elsewhere("Ana’s Mac") }

    @Test
    fun androidComponentUnplayableLineOnAPhone() = component("android.component.unplayable-line.on-a-phone") { Elsewhere("Ben’s phone") }
}

@Composable
private fun Elsewhere(device: String) =
    AndroidUnplayableLine(BridgeMessage("core.release.on_device", listOf(BridgeMessageArg("device", device))))
