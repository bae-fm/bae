package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import fm.bae.app.ui.appearance.ThemeSpace

/** A checkbox row with its [label]; the whole row toggles. */
@Composable
fun AndroidCheckbox(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .toggleable(
                    value = checked,
                    role = Role.Checkbox,
                    onValueChange = onCheckedChange,
                ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = checked, onCheckedChange = null)
        Spacer(modifier = Modifier.width(ThemeSpace.related))
        Text(label)
    }
}
