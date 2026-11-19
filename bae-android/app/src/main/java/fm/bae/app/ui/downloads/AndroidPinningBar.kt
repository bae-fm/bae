package fm.bae.app.ui.downloads

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidTopBar
import fm.bae.app.ui.components.ButtonKind

/** The Pinning screen's top bar: back, Retry for the failed pins, and Pause or Resume for the queue. */
@Composable
internal fun AndroidPinningBar(
    paused: Boolean,
    hasDownloads: Boolean,
    hasFailures: Boolean,
    onBack: () -> Unit,
    onPauseToggle: () -> Unit,
    onRetry: () -> Unit,
) {
    AndroidTopBar(onBack = onBack, title = stringResource(R.string.pinning)) {
        AndroidButton(kind = ButtonKind.TEXT, onClick = onRetry, enabled = hasFailures) {
            Text(stringResource(R.string.retry))
        }
        AndroidButton(kind = ButtonKind.TEXT, onClick = onPauseToggle, enabled = hasDownloads) {
            Text(stringResource(if (paused) R.string.resume else R.string.pause))
        }
    }
}
