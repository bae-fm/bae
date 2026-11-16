package fm.bae.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import fm.bae.app.R
import fm.bae.app.number
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import uniffi.bae_bridge.BridgeStars

/**
 * Five stars filled in the accent up to [value], the rest drawn as outlines. With [onRate] each star is
 * a button that rates with that many stars.
 */
@Composable
fun Stars(
    value: Int,
    size: Dp,
    modifier: Modifier = Modifier,
    onRate: ((BridgeStars) -> Unit)? = null,
) {
    val valueDescription =
        if (value == 0) stringResource(R.string.stars_none) else stringResource(R.string.stars_of_five, value)
    Row(
        modifier =
            modifier.semantics(mergeDescendants = onRate == null) {
                if (onRate == null) contentDescription = valueDescription
            },
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.line),
    ) {
        for (stars in BridgeStars.entries) {
            val label = stringResource(R.string.stars_rate, stars.number)
            Star(
                filled = stars.number <= value,
                size = size,
                modifier =
                    if (onRate == null) {
                        Modifier
                    } else {
                        Modifier
                            .clickable(role = Role.Button, onClick = { onRate(stars) })
                            .semantics { contentDescription = label }
                    },
            )
        }
    }
}

/** The design system's star as path data, on a 24 by 24 grid. */
private const val STAR_PATH = "M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7L12 17.3 5.8 20.9l1.6-7L2 9.2l7.1-.6z"
private const val STAR_GRID = 24f
private const val STAR_STROKE = 1.5f

/** One star, drawn on the design system's grid. */
@Composable
private fun Star(
    filled: Boolean,
    size: Dp,
    modifier: Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val outline = BaeTheme.colors.hairlineStrong
    val star = remember { PathParser().parsePathString(STAR_PATH).toPath() }
    Canvas(modifier = modifier.size(size)) {
        val scale = this.size.width / STAR_GRID
        val path =
            Path().apply {
                addPath(star)
                transform(Matrix().apply { scale(scale, scale) })
            }
        if (filled) drawPath(path, accent, style = Fill)
        drawPath(
            path,
            if (filled) accent else outline,
            style = Stroke(width = STAR_STROKE * scale, join = StrokeJoin.Round),
        )
    }
}
