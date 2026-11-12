package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import fm.bae.app.ui.appearance.ThemeSpace

/**
 * A dialog's title with its [message] under it, 8 apart: the supporting text
 * Material sets under a headline, not content a section-step below. It draws
 * its own text styles, so a screenshot can draw it outside a dialog's window.
 */
@Composable
fun DialogHead(
    title: String?,
    message: String?,
) {
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        title?.let {
            Text(text = it, style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.onSurface)
        }
        message?.let {
            Text(
                text = it,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** What a dialog says and offers: its [title], [message] and [content], and its buttons. */
internal class DialogParts(
    val confirmButton: @Composable () -> Unit,
    val dismissButton: @Composable (() -> Unit)? = null,
    val title: String? = null,
    val message: String? = null,
    val content: @Composable (() -> Unit)? = null,
)

/**
 * Material's [AlertDialog] with the line under the title set as the dialog's
 * [message] (see [DialogHead]); [content] is whatever follows it.
 */
@Composable
fun AndroidDialog(
    onDismissRequest: () -> Unit,
    confirmButton: @Composable () -> Unit,
    dismissButton: @Composable (() -> Unit)? = null,
    title: String? = null,
    message: String? = null,
    content: @Composable (() -> Unit)? = null,
) {
    AndroidDialog(onDismissRequest, DialogParts(confirmButton, dismissButton, title, message, content))
}

/** The dialog [parts] in a window of their own. */
@Composable
internal fun AndroidDialog(
    onDismissRequest: () -> Unit,
    parts: DialogParts,
) {
    AlertDialog(
        onDismissRequest = onDismissRequest,
        title =
            if (parts.title != null || parts.message != null) ({ DialogHead(parts.title, parts.message) }) else null,
        text = parts.content,
        confirmButton = parts.confirmButton,
        dismissButton = parts.dismissButton,
    )
}
