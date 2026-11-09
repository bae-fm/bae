package fm.bae.app.ui.library

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.NoticeLook

@Composable
internal fun ErrorBanner(
    error: DisplayError,
    onRetry: (() -> Unit)? = null,
) {
    ErrorNotice(
        error = error,
        modifier = Modifier.padding(horizontal = ThemeSpace.edge),
        action = onRetry?.let { ErrorAction(stringResource(R.string.retry), onClick = it) },
        look = NoticeLook.QUIET,
    )
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
