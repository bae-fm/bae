package fm.bae.app.ui.components

import androidx.compose.material3.DividerDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp

/**
 * A line across a screen or list between what is above and what is below, one hairline thick unless
 * [thickness] says.
 */
@Composable
internal fun AndroidDivider(
    modifier: Modifier = Modifier,
    thickness: Dp = DividerDefaults.Thickness,
    color: Color = DividerDefaults.color,
) {
    HorizontalDivider(modifier = modifier, thickness = thickness, color = color)
}
