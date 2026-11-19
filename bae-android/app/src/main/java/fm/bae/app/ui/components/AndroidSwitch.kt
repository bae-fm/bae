package fm.bae.app.ui.components

import androidx.compose.material3.Switch
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/**
 * A setting's on or off switch, at the end of an [AndroidRow]. With no [onCheckedChange] it only shows
 * [checked], for a row that takes the tap itself.
 */
@Composable
internal fun AndroidSwitch(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    Switch(checked = checked, onCheckedChange = onCheckedChange, modifier = modifier, enabled = enabled)
}
