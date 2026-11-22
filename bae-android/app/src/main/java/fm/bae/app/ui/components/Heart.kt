package fm.bae.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme

/**
 * The rating of a library whose rating system is Heart, read-only: one heart, filled in the accent when [on] (the
 * person gave three stars or more) and an outline otherwise. An outline heart says nothing to screen readers.
 */
@Composable
fun Heart(
    on: Boolean,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val outline = BaeTheme.colors.hairlineStrong
    val heart = remember { PathParser().parsePathString(HEART_PATH).toPath() }
    val description = if (on) stringResource(R.string.heart_on) else null
    Canvas(
        modifier =
            modifier.size(size).semantics {
                description?.let { contentDescription = it }
            },
    ) {
        val scale = this.size.width / HEART_GRID
        val path =
            Path().apply {
                addPath(heart)
                transform(Matrix().apply { scale(scale, scale) })
            }
        if (on) drawPath(path, accent, style = Fill)
        drawPath(
            path,
            if (on) accent else outline,
            style = Stroke(width = HEART_STROKE * scale, join = StrokeJoin.Round),
        )
    }
}

/** The design system's heart as path data, on a 24 by 24 grid. */
private const val HEART_PATH =
    "M12 20.5s-7.6-4.6-9.5-9.3C1.1 7.8 3.3 4.3 6.8 4.3c2 0 3.6 1 5.2 2.9 1.6-1.9 3.2-2.9 5.2-2.9 " +
        "3.5 0 5.7 3.5 4.3 6.9-1.9 4.7-9.5 9.3-9.5 9.3z"
private const val HEART_GRID = 24f
private const val HEART_STROKE = 1.5f
