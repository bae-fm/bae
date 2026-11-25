package fm.bae.app.ui.components

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace

/** A centred notice's title colour: the stage's text on the stage, primary over advice, else secondary. */
@Composable
internal fun centeredTitleColor(
    stage: Boolean,
    hasAdvice: Boolean,
): Color =
    when {
        stage -> BaeTheme.colors.onFillSecondary
        hasAdvice -> MaterialTheme.colorScheme.onSurface
        else -> MaterialTheme.colorScheme.onSurfaceVariant
    }

/**
 * A centred notice's space: the width it is given and, except on the stage, all its height, at least [CENTERED_MIN]
 * tall, and padded: a stage notice is as tall as its words.
 */
internal fun Modifier.centeredFill(stage: Boolean): Modifier =
    if (stage) {
        fillMaxWidth()
    } else {
        fillMaxSize().heightIn(min = CENTERED_MIN).padding(ThemeSpace.section)
    }

/** What a centred notice stands at least this tall, where its space is unbounded. */
private val CENTERED_MIN = 300.dp
