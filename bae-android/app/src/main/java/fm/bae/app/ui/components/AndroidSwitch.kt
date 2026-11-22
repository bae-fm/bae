package fm.bae.app.ui.components

import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Switch
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role

/** The action and indicator of one switch, rendered together by their container. */
internal class SwitchControl(
    val action: Modifier,
    val indicator: @Composable () -> Unit,
)

@Composable
internal fun AndroidSwitch(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    content: @Composable (SwitchControl) -> Unit,
) {
    content(
        SwitchControl(
            action =
                Modifier.toggleable(
                    value = checked,
                    role = Role.Switch,
                    onValueChange = onCheckedChange,
                ),
            indicator = { Switch(checked = checked, onCheckedChange = null) },
        ),
    )
}
