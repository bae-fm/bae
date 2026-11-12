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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import fm.bae.app.R
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
        AndroidSpinner(size = size, description = text)
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
 * fill, where the default does not read. Screen readers name it [description].
 */
@Composable
internal fun AndroidSpinner(
    modifier: Modifier = Modifier,
    size: Dp = ThemeIcon.large,
    color: Color = ProgressIndicatorDefaults.circularColor,
    description: String = stringResource(R.string.progress_loading),
) {
    CircularProgressIndicator(
        modifier = modifier.size(size).semantics { contentDescription = description },
        color = color,
        strokeWidth = size * STROKE_SHARE,
        trackColor = color.copy(alpha = TRACK_ALPHA),
    )
}

/** The ring's width as a share of the spinner's size: the board draws 2.5 in 24. */
private const val STROKE_SHARE = 2.5f / 24f

/** The faint full ring behind the turning arc, as the design draws it. */
private const val TRACK_ALPHA = 0.2f
