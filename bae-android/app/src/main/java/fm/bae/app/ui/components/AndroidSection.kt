package fm.bae.app.ui.components

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * The heading over a card of rows: sentence case, in the accent colour, as Material's list section header and the
 * design system's `AndroidSection` draw it. It sits in a column already inset by [ThemeSpace.edge] and spaced by
 * [ThemeSpace.related], so it adds only what lines its text up with the rows' text ([ThemeSpace.page] from the screen's
 * edge) and the rest of the gap that sets a section apart.
 */
@Composable
fun AndroidSection(
    text: String,
    modifier: Modifier = Modifier,
) {
    Text(
        text = text,
        style = ThemeText.section.style,
        color = MaterialTheme.colorScheme.primary,
        modifier =
            modifier.padding(
                start = ThemeSpace.page - ThemeSpace.edge,
                end = ThemeSpace.page - ThemeSpace.edge,
                top = ThemeSpace.section - ThemeSpace.related,
            ),
    )
}
