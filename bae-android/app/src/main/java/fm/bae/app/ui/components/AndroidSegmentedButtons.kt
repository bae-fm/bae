package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * Material's single-choice segmented buttons, full width, the [selected] one checked. [label] names the choice over
 * them, in the secondary text, as a form does; a choice a screen names itself passes none.
 */
@Composable
internal fun AndroidSegmentedButtons(
    options: List<String>,
    selected: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    label: String? = null,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        label?.let {
            Text(
                text = it,
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        val edge = BaeTheme.colors.hairlineStrong
        val text = MaterialTheme.colorScheme.onSurface
        // The chosen segment is the accent's tint beside its check; the pill is outlined in the strong hairline.
        val colors =
            SegmentedButtonDefaults.colors(
                activeContainerColor = MaterialTheme.colorScheme.primary.copy(alpha = ThemeOpacity.tint),
                activeContentColor = text,
                activeBorderColor = edge,
                inactiveContainerColor = Color.Transparent,
                inactiveContentColor = text,
                inactiveBorderColor = edge,
            )
        SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
            options.forEachIndexed { index, option ->
                SegmentedButton(
                    selected = index == selected,
                    onClick = { onSelect(index) },
                    shape = SegmentedButtonDefaults.itemShape(index = index, count = options.size),
                    colors = colors,
                ) {
                    Text(option)
                }
            }
        }
    }
}
