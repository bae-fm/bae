package fm.bae.app.ui.onboarding

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
import fm.bae.app.ui.components.DialogParts
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
    AndroidDialog(onDismissRequest = onCancel, parts = removalFailedParts(library, error, onRetry, onCancel))
}

/**
 * What the dialog for a failed removal of [library] says and offers: the title naming it, [error]'s line,
 * and *Retry* with *Cancel*, or *OK* alone when retrying can't get past the error.
 */
@Composable
internal fun removalFailedParts(
    library: BridgeLibrary,
    error: DisplayError,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
): DialogParts {
    val title = stringResource(R.string.library_remove_failed_title, library.name)
    return if (error.offersRetry) {
        DialogParts(
            title = title,
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
        DialogParts(
            title = title,
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

@Preview(showBackground = true)
@Composable
private fun RemovalFailedPreview() {
    BaeTheme {
        val parts =
            removalFailedParts(
                library = PreviewData.library(id = "lib-1", name = "Home music", isActive = false),
                error = DisplayError("Could not reach the cloud."),
                onRetry = {},
                onCancel = {},
            )
        DialogHead(parts.title, parts.message)
    }
}
