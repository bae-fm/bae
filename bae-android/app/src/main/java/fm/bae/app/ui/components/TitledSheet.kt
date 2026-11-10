package fm.bae.app.ui.components

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace

/**
 * A [ContentSheet] for something to look at with nothing to decide, titled by the piece it describes.
 * The title sits where the rows' text does, 24dp in.
 */
@Composable
internal fun TitledSheet(
    title: String,
    onDismiss: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    ContentSheet(onDismiss = onDismiss) {
        TitledSheetContent(title, content)
    }
}

/**
 * The title, then [content], as a [TitledSheet] lays them out inside its sheet. It is separate so a
 * screenshot can draw it with [ContentSheetBody]: a sheet opens a window the screenshot tests don't render.
 */
@Composable
internal fun ColumnScope.TitledSheetContent(
    title: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleLarge,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.group),
    )
    content()
}
