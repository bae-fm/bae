package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * One choice of several in a dialog: Material's radio button with its [label],
 * the whole row selectable. Exactly one of a dialog's rows is [checked].
 */
@Composable
internal fun AndroidRadio(
    label: String,
    checked: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .defaultMinSize(minHeight = MIN_HEIGHT)
                .selectable(selected = checked, role = Role.RadioButton, onClick = onClick),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = checked, onClick = null)
        Text(text = label, style = ThemeText.rowTitle.style)
    }
}

private val MIN_HEIGHT = 48.dp

@Preview(showBackground = true)
@Composable
private fun AndroidRadioPreview() {
    BaeTheme {
        Column {
            AndroidRadio(label = "Off", checked = false, onClick = {})
            AndroidRadio(label = "Per track", checked = false, onClick = {})
            AndroidRadio(label = "Per album", checked = true, onClick = {})
        }
    }
}
