package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
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
        AlertDialog(
            onDismissRequest = onCancel,
            text = { RemovalFailedText(library, error) },
            confirmButton = { TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) } },
            dismissButton = { TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) } },
        )
    } else {
        AlertDialog(
            onDismissRequest = onCancel,
            text = { RemovalFailedText(library, error) },
            confirmButton = { TextButton(onClick = onCancel) { Text(stringResource(android.R.string.ok)) } },
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
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
        Text(
            text = stringResource(R.string.library_remove_failed_title, library.name),
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Text(error.line, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
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
