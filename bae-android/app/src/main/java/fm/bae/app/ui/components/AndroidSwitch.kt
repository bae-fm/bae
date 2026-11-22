package fm.bae.app.ui.components

import androidx.compose.material3.Switch
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics

/** The name and action of a switch that handles its own taps. */
internal class SwitchAction(
    val label: String,
    val set: (Boolean) -> Unit,
)

/** A switch. When its setting row handles taps, this draws the indicator for that row. */
@Composable
internal fun AndroidSwitch(
    checked: Boolean,
    action: SwitchAction? = null,
) {
    val modifier = if (action == null) Modifier else Modifier.semantics { contentDescription = action.label }
    Switch(checked = checked, onCheckedChange = action?.set, modifier = modifier)
}
