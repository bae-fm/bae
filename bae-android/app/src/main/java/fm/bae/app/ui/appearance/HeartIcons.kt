package fm.bae.app.ui.appearance

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

/**
 * The heart a rating shows, drawn as the design system's `Heart` draws it: one path across a 24-unit box, outlined
 * with a stroke 1.5 units wide and rounded at its joins, and filled too when the person hearted.
 */
internal object HeartIcons {
    val Filled: ImageVector by lazy { heart("Filled", filled = true) }

    val Outline: ImageVector by lazy { heart("Outline", filled = false) }
}

private const val HEART =
    "M12 20.5s-7.6-4.6-9.5-9.3C1.1 7.8 3.3 4.3 6.8 4.3c2 0 3.6 1 5.2 2.9 1.6-1.9 3.2-2.9 5.2-2.9 3.5 0 5.7 3.5 " +
        "4.3 6.9-1.9 4.7-9.5 9.3-9.5 9.3z"

private const val HEART_STROKE = 1.5f

private fun heart(
    name: String,
    filled: Boolean,
): ImageVector =
    ImageVector
        .Builder(name = name, defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
        .apply {
            addPath(
                pathData = PathParser().parsePathString(HEART).toNodes(),
                fill = if (filled) SolidColor(Color.Black) else null,
                stroke = SolidColor(Color.Black),
                strokeLineWidth = HEART_STROKE,
                strokeLineJoin = StrokeJoin.Round,
            )
        }.build()
