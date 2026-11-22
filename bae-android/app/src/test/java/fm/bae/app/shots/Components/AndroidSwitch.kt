package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.selection.toggleable
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import fm.bae.app.ui.components.AndroidSwitch
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
    Box(
        Modifier
            .toggleable(value = checked, role = Role.Switch, onValueChange = {})
            .semantics { contentDescription = "Restore on launch" },
    ) {
        AndroidSwitch(checked = checked, enabled = true)
    }
}
