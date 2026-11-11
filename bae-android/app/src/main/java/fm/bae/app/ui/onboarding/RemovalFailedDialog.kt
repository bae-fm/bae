package fm.bae.app.ui.onboarding

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogHead
import uniffi.bae_bridge.BridgeLibrary

/**
 * The dialog for a removal of [library] that failed, or the count before it:
 * the error's line, *Retry*, which runs it again, and *Cancel*, which leaves the
 * library as it was; *OK* alone when core says retrying can't get past it.
 */
@Composable
internal fun RemovalFailedDialog(
    library: BridgeLibrary,
    error: DisplayError,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
) {
    if (error.offersRetry) {
        AndroidDialog(
            onDismissRequest = onCancel,
            title = stringResource(R.string.library_remove_failed_title, library.name),
            message = error.line,
            confirmButton = {
                AndroidButton(
                    kind = ButtonKind.TEXT,
                    onClick = onRetry,
                ) { Text(stringResource(R.string.retry)) }
            },
            dismissButton = {
                AndroidButton(
                    kind = ButtonKind.TEXT,
                    tone = ButtonTone.NEUTRAL,
                    onClick = onCancel,
                ) { Text(stringResource(R.string.cancel)) }
            },
        )
    } else {
        AndroidDialog(
            onDismissRequest = onCancel,
            title = stringResource(R.string.library_remove_failed_title, library.name),
            message = error.line,
            confirmButton = {
                AndroidButton(
                    kind = ButtonKind.TEXT,
                    onClick = onCancel,
                ) { Text(stringResource(android.R.string.ok)) }
            },
        )
    }
}

/**
 * The dialog's words: the title naming [library], then [error]'s line. It is
 * separate from the dialog so a screenshot can draw it: an [AlertDialog] opens
 * a window the screenshot tests don't render.
 */
@Composable
internal fun RemovalFailedText(
    library: BridgeLibrary,
    error: DisplayError,
) {
    DialogHead(stringResource(R.string.library_remove_failed_title, library.name), error.line)
}

@Preview(showBackground = true)
@Composable
private fun RemovalFailedPreview() {
    BaeTheme {
        RemovalFailedText(
            library = PreviewData.library(id = "lib-1", name = "Home music", isActive = false),
            error = DisplayError("Couldn’t reach the cloud."),
        )
    }
}
