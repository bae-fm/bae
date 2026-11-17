package fm.bae.app.ui.library

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.takeOrElse
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import kotlin.math.ceil

/** The narrowest an album card is: the album grid's adaptive column width. */
internal val ALBUM_CARD_MIN_WIDTH: Dp = 150.dp

/**
 * How many library rows the screen fits, before any row is drawn: the screen's height over the shortest row, one
 * line of title text between the row's padding, and one more row for the rows the edges cut. A list opens its view
 * with that many rows on screen.
 */
@Composable
internal fun rememberScreenRows(): Int {
    val height = LocalConfiguration.current.screenHeightDp.dp
    val style = ThemeText.rowTitle.style
    val line = with(LocalDensity.current) { style.lineHeight.takeOrElse { style.fontSize }.toDp() }
    val row = line + ThemeSpace.related * 2
    return ceil(height / row).toInt() + 1
}

/**
 * How many album cards the screen fits, before any card is drawn: every column at the screen's width, times the
 * rows of cards whose cover is a column wide, and one more row for the rows the edges cut.
 */
@Composable
internal fun rememberScreenSlots(): Int {
    val configuration = LocalConfiguration.current
    val columns = maxOf(1, (configuration.screenWidthDp.dp / ALBUM_CARD_MIN_WIDTH).toInt())
    val rows = ceil(configuration.screenHeightDp.dp / ALBUM_CARD_MIN_WIDTH).toInt() + 1
    return rows * columns
}
