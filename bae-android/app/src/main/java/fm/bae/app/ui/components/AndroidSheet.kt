package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace

/**
 * A bottom sheet that holds [content] as tall as it is, up to near the top of the screen, under
 * the [title] that names it when it has one. Swiping it down or tapping outside it closes it, so
 * it has no Close button. The sheet insets its content by `ThemeSpace.related`, so rows that inset
 * themselves by `ThemeSpace.edge` put their text 24dp in, where the title sits too.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AndroidSheet(
    onDismiss: () -> Unit,
    title: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        AndroidSheetBody(title, content)
    }
}

/**
 * What an [AndroidSheet] holds, with the sheet's insets. It is separate so a shot can draw it: a
 * [ModalBottomSheet] opens a window the shots don't render.
 */
@Composable
internal fun AndroidSheetBody(
    title: String?,
    content: @Composable ColumnScope.() -> Unit,
) {
    val bottomInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    Column(
        modifier =
            Modifier.padding(
                start = ThemeSpace.related,
                end = ThemeSpace.related,
                bottom = bottomInset + ThemeSpace.section,
            ),
    ) {
        title?.let {
            Text(
                text = it,
                style = MaterialTheme.typography.titleLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.group),
            )
        }
        content()
    }
}
