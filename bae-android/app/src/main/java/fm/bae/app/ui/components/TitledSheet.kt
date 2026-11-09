package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
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
 * A bottom sheet for something to look at with nothing to decide, titled by the
 * piece it describes. Swiping it down or tapping outside it closes it, so it has
 * no Close button.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun TitledSheet(
    title: String,
    onDismiss: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState()) {
        TitledSheetContent(title, content)
    }
}

/**
 * The body of a [TitledSheet]: the title, then [content]. It is separate from
 * the sheet so a screenshot can draw it: a [ModalBottomSheet] opens a window the
 * screenshot tests don't render.
 */
@Composable
internal fun TitledSheetContent(
    title: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    val bottomInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    Column(modifier = Modifier.fillMaxWidth().padding(bottom = bottomInset + ThemeSpace.edge)) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.group),
        )
        content()
    }
}
