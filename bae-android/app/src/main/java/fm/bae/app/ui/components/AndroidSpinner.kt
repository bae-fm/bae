package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ProgressIndicatorDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * A wait with no length and no steps of its own, such as a list still filling:
 * the spinner, with [text] beside it, the words that say what is happening. A
 * wait whose length is known, or a step of a flow, draws a bar instead.
 */
@Composable
internal fun AndroidSpinner(
    text: String,
    modifier: Modifier = Modifier,
    size: Dp = ThemeIcon.large,
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AndroidSpinner(size = size)
        Text(
            text = text,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * The spinner alone, where what it waits on is said elsewhere: [size] is the
 * glyph size of the control it sits in, and [color] is set over an image or a
 * fill, where the default does not read.
 */
@Composable
internal fun AndroidSpinner(
    modifier: Modifier = Modifier,
    size: Dp = ThemeIcon.large,
    color: Color = ProgressIndicatorDefaults.circularColor,
) {
    CircularProgressIndicator(modifier = modifier.size(size), color = color, strokeWidth = 2.dp)
}

@Preview(showBackground = true)
@Composable
private fun AndroidSpinnerPreview() {
    BaeTheme {
        AndroidSpinner(text = "Making your restore code…")
    }
}
