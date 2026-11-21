package fm.bae.app.ui.appearance

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

/**
 * The glyphs a notice leads with: a ring, 20 of the 24 units across, with a solid mark inside it. The
 * design system draws every notice's glyph this way (`STATUS_RING` and `STATUS_MARK` in its shared
 * components), on Android as on the Mac, so these are the same paths.
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

/** The ring with [mark] inside it, 24dp across, in the colour a caller tints it. */
private fun ring(
    name: String,
    mark: String,
): ImageVector =
    ImageVector
        .Builder(name = name, defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
        .apply {
            addPath(
                pathData = PathParser().parsePathString(RING).toNodes(),
                pathFillType = PathFillType.EvenOdd,
                fill = SolidColor(Color.Black),
            )
            addPath(pathData = PathParser().parsePathString(mark).toNodes(), fill = SolidColor(Color.Black))
        }.build()
