package fm.bae.app.ui.components

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeText

/** The small label in the secondary colour that names a side, a disc, a group or a column, in sentence case. */
@Composable
fun AndroidEyebrow(
    text: String,
    modifier: Modifier = Modifier,
) {
    Text(
        text = text,
        style = ThemeText.androidEyebrow.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = modifier,
    )
}
