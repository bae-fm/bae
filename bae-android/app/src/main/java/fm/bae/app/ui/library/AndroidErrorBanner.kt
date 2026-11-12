package fm.bae.app.ui.library

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction

@Composable
internal fun AndroidErrorBanner(
    title: String,
    error: DisplayError,
    onRetry: (() -> Unit)? = null,
) {
    AndroidNotice(
        title = title,
        advice = null,
        error = error,
        modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        action = onRetry?.let { error.retryAction(onClick = it) },
        look = NoticeLook.QUIET,
    )
}

@Preview(showBackground = true)
@Composable
private fun AndroidErrorBannerPreview() {
    PreviewTheme {
        AndroidErrorBanner(
            title = "Could not read more albums.",
            error = DisplayError("Couldn't reach the library.", "database error: disk I/O error"),
            onRetry = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AndroidErrorBannerNoRetryPreview() {
    PreviewTheme {
        AndroidErrorBanner(
            title = "Could not read more albums.",
            error = DisplayError("Couldn't reach the library."),
        )
    }
}
