package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Switch
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics

/**
 * One switch action and its Material indicator. A setting row puts the action on its whole surface and uses
 * its visible title as the name. On its own, the switch takes a [label].
 */
@Composable
internal fun AndroidSwitch(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    enabled: Boolean = true,
    label: String? = null,
    layout: @Composable (Modifier, @Composable () -> Unit) -> Unit = { action, indicator ->
        Box(action) { indicator() }
    },
) {
    val action =
        Modifier.toggleable(
            value = checked,
            enabled = enabled,
            role = Role.Switch,
            onValueChange = onCheckedChange,
        )
    val named = if (label == null) action else action.semantics { contentDescription = label }
    layout(named) { Switch(checked = checked, onCheckedChange = null) }
}
