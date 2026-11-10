package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * The step a call runs, named by [title], over a bar of how much of it is
 * done: [fraction] of it, or moving without a count when it is null.
 */
@Composable
internal fun StepBar(
    title: String,
    fraction: Float?,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.widthIn(max = 360.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
    ) {
        Text(
            text = title,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        if (fraction == null) {
            LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
        } else {
            LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth())
        }
    }
}

/** [done] of [total] as a bar's fraction, or null when [total] counts nothing. */
internal fun stepFraction(
    done: ULong,
    total: ULong,
): Float? = if (total == 0uL) null else minOf(done, total).toFloat() / total.toFloat()

@Preview(showBackground = true)
@Composable
private fun StepBarPreview() {
    BaeTheme {
        StepBar(title = "Connecting and checking your cloud home…", fraction = null)
    }
}
