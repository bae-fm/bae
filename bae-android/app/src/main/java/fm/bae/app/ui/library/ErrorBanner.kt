package fm.bae.app.ui.library

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Notice
import fm.bae.app.ui.components.StatusTone

@Composable
internal fun ErrorBanner(
    message: String,
    onRetry: (() -> Unit)? = null,
) {
    Notice(
        tone = StatusTone.DANGER,
        modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
    ) {
        Text(text = message, style = ThemeText.body.style, modifier = Modifier.weight(1f))
        if (onRetry != null) {
            TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ErrorBannerPreview() {
    BaeTheme {
        ErrorBanner(message = "Couldn't reach the library.", onRetry = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun ErrorBannerNoRetryPreview() {
    BaeTheme {
        ErrorBanner(message = "Couldn't reach the library.")
    }
}
