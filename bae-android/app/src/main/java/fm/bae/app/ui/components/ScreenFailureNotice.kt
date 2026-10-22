package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import fm.bae.app.ScreenFailure
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/** A failure as a screen shows it: what failed, the error with its details, and what to do next. */
@Composable
fun ScreenFailureNotice(
    failure: ScreenFailure,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
    ) {
        Text(
            text = context.coreString(failure.step.titleKey),
            style = ThemeText.heading.style,
            textAlign = TextAlign.Center,
        )
        ErrorDetailDisclosure(error = failure.error)
        failure.step.adviceKey?.let { key ->
            Text(
                text = context.coreString(key),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
    }
}
