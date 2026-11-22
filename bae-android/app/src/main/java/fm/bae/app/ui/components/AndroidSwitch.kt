package fm.bae.app.ui.components

import androidx.compose.material3.Switch
import androidx.compose.runtime.Composable

/** The switch at the end of an [AndroidRow]. The row handles taps. */
@Composable
internal fun AndroidSwitch(
    checked: Boolean,
    enabled: Boolean,
) {
    Switch(checked = checked, onCheckedChange = null, enabled = enabled)
}
