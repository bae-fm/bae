package fm.bae.app.ui.components

import android.view.WindowManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
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

/** The width, padding included, that the design system gives a dialog whose board states none. */
private val DIALOG_WIDTH = 312.dp

/** The width, padding included, that the boards give most dialogs that state one. */
internal val WIDE_DIALOG_WIDTH = 340.dp

/** The width, padding included, that the boards of a device removal and a device reset give their dialogs. */
internal val NARROW_DIALOG_WIDTH = 320.dp

/**
 * One button of a dialog's last row: its [label], what a tap does, and whether it takes a tap now. An
 * action is [destructive] when it loses something, and [filled] when it is what the dialog is for, such as
 * Play after a side ends.
 */
internal class DialogAction(
    val label: String,
    val onClick: () -> Unit,
    val enabled: Boolean = true,
    val destructive: Boolean = false,
    val filled: Boolean = false,
)

/**
 * A dialog's last row, at its right: [dismiss] (Cancel) first, then the [action], in the accent, in the
 * danger colour when destructive, or as the accent's filled button when filled. Beside an action, Cancel is
 * in the text colour, so two coloured buttons never sit side by side; alone, as under a list of choices, it
 * keeps the accent.
 */
@Composable
internal fun AndroidDialogActions(
    action: DialogAction?,
    dismiss: DialogAction?,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related, Alignment.End)) {
        dismiss?.let {
            AndroidButton(
                kind = ButtonKind.TEXT,
                tone = if (action != null) ButtonTone.NEUTRAL else ButtonTone.DEFAULT,
                onClick = it.onClick,
                enabled = it.enabled,
            ) {
                Text(it.label)
            }
        }
        action?.let {
            AndroidButton(
                kind = if (it.filled) ButtonKind.FILLED else ButtonKind.TEXT,
                tone = if (it.destructive) ButtonTone.DESTRUCTIVE else ButtonTone.DEFAULT,
                onClick = it.onClick,
                enabled = it.enabled,
            ) {
                Text(it.label)
            }
        }
    }
}

/**
 * What a dialog says and offers: its [title], [message] and [content], its [action] and [dismiss] buttons,
 * and the [width] it holds.
 */
internal class DialogParts(
    val action: DialogAction? = null,
    val dismiss: DialogAction? = null,
    val title: String? = null,
    val message: String? = null,
    val content: @Composable (() -> Unit)? = null,
    val width: Dp = DIALOG_WIDTH,
)

/**
 * Material's [AlertDialog] with the line under the title set as the dialog's
 * [message] (see [DialogHead]); [content] is whatever follows it, and [AndroidDialogActions] ends it. A
 * dialog is [width] wide, padding included, which is 312 unless a board draws it otherwise.
 */
@Composable
internal fun AndroidDialog(
    onDismissRequest: () -> Unit,
    action: DialogAction? = null,
    dismiss: DialogAction? = null,
    title: String? = null,
    message: String? = null,
    content: @Composable (() -> Unit)? = null,
    width: Dp = DIALOG_WIDTH,
) {
    AndroidDialog(onDismissRequest, DialogParts(action, dismiss, title, message, content, width))
}

/** The dialog [parts] in a window of their own. */
@Composable
internal fun AndroidDialog(
    onDismissRequest: () -> Unit,
    parts: DialogParts,
) {
    AlertDialog(
        onDismissRequest = onDismissRequest,
        modifier = Modifier.width(parts.width),
        title =
            if (parts.title != null || parts.message != null) ({ DialogHead(parts.title, parts.message) }) else null,
        text = parts.content,
        confirmButton = {
            DialogScrim(BaeTheme.colors.scrim)
            AndroidDialogActions(parts.action, parts.dismiss)
        },
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
