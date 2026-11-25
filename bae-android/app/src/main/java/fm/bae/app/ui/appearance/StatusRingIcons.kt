package fm.bae.app.ui.appearance

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

/**
 * The glyphs a notice leads with: a ring with a solid mark inside it, drawn across the ring's own 20 units as the
 * design system draws them (`RING_DRAWING` in its shared components, whose `viewBox` is `2 2 20 20`), so an icon is
 * as wide as its ring.
 */
internal object StatusRingIcons {
    val Info: ImageVector by lazy {
        ring("Info", "M10.9 10h2.2v7.5h-2.2zM12 5.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z")
    }

    val Success: ImageVector by lazy { ring("Success", "M10.4 16.6 6.4 12.6l1.5-1.5 2.5 2.5 5.7-5.7 1.5 1.5z") }

    /** The exclamation mark of a warning and of a failure. */
    val Bang: ImageVector by lazy {
        ring("Bang", "M10.9 6.5h2.2l-.3 8h-1.6zM12 15.8a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 1 0 0-2.6z")
    }
}

private const val RING = "M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20zM12 3.6a8.4 8.4 0 1 1 0 16.8 8.4 8.4 0 1 1 0-16.8z"

/** The ring with [mark] inside it, 20dp across, in the colour a caller tints it. */
private fun ring(
    name: String,
    mark: String,
): ImageVector =
    ImageVector
        .Builder(name = name, defaultWidth = 20.dp, defaultHeight = 20.dp, viewportWidth = 20f, viewportHeight = 20f)
        .apply {
            // The paths are drawn on a 24-unit grid with the ring at 2 to 22.
            addGroup(translationX = -2f, translationY = -2f)
            addPath(
                pathData = PathParser().parsePathString(RING).toNodes(),
                pathFillType = PathFillType.EvenOdd,
                fill = SolidColor(Color.Black),
            )
            addPath(pathData = PathParser().parsePathString(mark).toNodes(), fill = SolidColor(Color.Black))
        }.build()
