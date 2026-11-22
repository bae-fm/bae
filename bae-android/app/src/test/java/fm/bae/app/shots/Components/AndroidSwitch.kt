package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fm.bae.app.ui.components.AndroidSwitch
import fm.bae.app.ui.components.SwitchAction
import org.junit.Test

/** The component shots of `AndroidSwitch`, one for each row of its preview. */
class AndroidSwitchShots : BoardShot() {
    @Test
    fun androidComponentSwitchOn() = component("android.component.switch.on") { RestoreOnLaunch(checked = true) }

    @Test
    fun androidComponentSwitchOff() = component("android.component.switch.off") { RestoreOnLaunch(checked = false) }
}

/** The switch as the preview draws it, named for its setting. */
@Composable
private fun RestoreOnLaunch(checked: Boolean) {
    var on by remember { mutableStateOf(checked) }
    AndroidSwitch(checked = on, action = SwitchAction("Restore on launch") { on = it })
}
