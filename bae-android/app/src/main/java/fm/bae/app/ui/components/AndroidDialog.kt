package fm.bae.app.ui.components

import android.view.WindowManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.window.DialogWindowProvider
import fm.bae.app.ui.BaeTheme
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

/** What a dialog says and offers: its [title], [message] and [content], its buttons and the [width] it holds. */
internal class DialogParts(
    val confirmButton: @Composable () -> Unit,
    val dismissButton: @Composable (() -> Unit)? = null,
    val title: String? = null,
    val message: String? = null,
    val content: @Composable (() -> Unit)? = null,
    val width: Dp? = null,
)

/**
 * Material's [AlertDialog] with the line under the title set as the dialog's
 * [message] (see [DialogHead]); [content] is whatever follows it. A dialog is [width] wide when one
 * is given, else as wide as Material makes it for what it holds.
 */
@Composable
fun AndroidDialog(
    onDismissRequest: () -> Unit,
    confirmButton: @Composable () -> Unit,
    dismissButton: @Composable (() -> Unit)? = null,
    title: String? = null,
    message: String? = null,
    content: @Composable (() -> Unit)? = null,
    width: Dp? = null,
) {
    AndroidDialog(onDismissRequest, DialogParts(confirmButton, dismissButton, title, message, content, width))
}

/** The dialog [parts] in a window of their own. */
@Composable
internal fun AndroidDialog(
    onDismissRequest: () -> Unit,
    parts: DialogParts,
) {
    AlertDialog(
        onDismissRequest = onDismissRequest,
        modifier = parts.width?.let { Modifier.width(it) } ?: Modifier,
        title =
            if (parts.title != null || parts.message != null) ({ DialogHead(parts.title, parts.message) }) else null,
        text = parts.content,
        confirmButton = {
            DialogScrim(BaeTheme.colors.scrim)
            parts.confirmButton()
        },
        dismissButton = parts.dismissButton,
    )
}

/**
 * Dims the screen behind the dialog window with the design system's scrim. The window dims with black at the scrim's
 * opacity, which is the scrim colour laid over the screen. DialogScrim draws nothing itself and runs inside the
 * dialog's window.
 */
@Composable
private fun DialogScrim(scrim: Color) {
    val window = (LocalView.current.parent as DialogWindowProvider).window
    SideEffect {
        window.addFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        window.setDimAmount(scrim.alpha)
    }
}
