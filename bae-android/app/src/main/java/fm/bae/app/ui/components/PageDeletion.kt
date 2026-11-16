package fm.bae.app.ui.components

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

private val logger = BaeLogger("bae.PageDeletion")

/**
 * Deleting what a page shows: what the open confirmation names, and why the last delete failed.
 * [operation] names the delete in the log. When [delete] succeeds, the holder calls [onDeleted] and
 * the page closes; when it fails, the page stays open and shows the error.
 */
internal class PageDeletion<Target : Any>(
    private val operation: String,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
    private val delete: suspend () -> Unit,
    private val onDeleted: () -> Unit,
) {
    /** What the confirmation names while it is open; null while it is closed. */
    var confirming by mutableStateOf<Target?>(null)
        private set

    var error by mutableStateOf<DisplayError?>(null)
        private set

    /** Show the confirmation, naming [target]. */
    fun ask(target: Target) {
        confirming = target
    }

    /** Close the confirmation without deleting. */
    fun dismiss() {
        confirming = null
    }

    /** Close the confirmation and delete. */
    fun confirm() {
        confirming = null
        error = null
        scope.launch {
            performBridgeAction(logger, operation, errors, { error = it }) {
                delete()
                onDeleted()
            }
        }
    }
}

/**
 * Remembers the deletion that [operation] names (such as "delete playlist <id>"), runs [delete] in
 * [scope] when the person confirms, and calls [onDeleted] when it succeeds.
 */
@Composable
internal fun <Target : Any> rememberPageDeletion(
    session: OpenLibrary,
    operation: String,
    scope: CoroutineScope,
    onDeleted: () -> Unit,
    delete: suspend () -> Unit,
): PageDeletion<Target> {
    val leave by rememberUpdatedState(onDeleted)
    return remember(operation, session, scope) {
        PageDeletion(operation, session.errors, scope, delete) { leave() }
    }
}

/**
 * Shows why the last delete failed, under the page's bar, and the [dialog] for what the open
 * confirmation names. The dialog's buttons call the two functions it receives: confirm, then dismiss.
 */
@Composable
internal fun <Target : Any> PageDeletionShown(
    deletion: PageDeletion<Target>,
    dialog: @Composable (target: Target, onConfirm: () -> Unit, onDismiss: () -> Unit) -> Unit,
) {
    deletion.error?.let {
        AndroidNotice(
            error = it,
            modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        )
    }
    deletion.confirming?.let { dialog(it, deletion::confirm, deletion::dismiss) }
}
