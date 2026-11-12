package fm.bae.app.ui.components

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * The small label in the secondary colour that names a side, a disc, a group or a column, in sentence
 * case, with [spaceAbove] over it to set it off from what comes before; a label that sits at the head
 * of its own row passes none.
 */
@Composable
fun AndroidEyebrow(
    text: String,
    modifier: Modifier = Modifier,
    spaceAbove: Dp = ThemeSpace.related,
) {
    Text(
        text = text,
        style = ThemeText.androidEyebrow.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = modifier.padding(top = spaceAbove),
    )
}
