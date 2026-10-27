package fm.bae.app.ui.library

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.Notice
import fm.bae.app.ui.components.StatusTone

@Composable
internal fun ErrorBanner(
    error: DisplayError,
    onRetry: (() -> Unit)? = null,
) {
    Notice(
        tone = StatusTone.DANGER,
        modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
    ) {
        ErrorDetailDisclosure(error = error, modifier = Modifier.weight(1f), lineColor = LocalContentColor.current)
        if (onRetry != null) {
            TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ErrorBannerPreview() {
    BaeTheme {
        ErrorBanner(
            error = DisplayError("Couldn't reach the library.", "database error: disk I/O error"),
            onRetry = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun ErrorBannerNoRetryPreview() {
    BaeTheme {
        ErrorBanner(error = DisplayError("Couldn't reach the library."))
    }
}
