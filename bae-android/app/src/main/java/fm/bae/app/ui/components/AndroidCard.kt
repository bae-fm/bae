package fm.bae.app.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.ParentDataModifier
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace

/**
 * Groups related [AndroidRow]s on a screen, as Android 16's Settings draws them: each child is its own
 * tile, [ThemeSpace.line] apart from the next, with only the group's outer corners rounded and no
 * dividers. A child that [joinsTileAbove] shares the tile above it, with no gap.
 */
@Composable
internal fun AndroidCard(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val color = MaterialTheme.colorScheme.surfaceContainer
    var tiles by remember { mutableStateOf(emptyList<Tile>()) }
    Layout(
        content = content,
        modifier = modifier.fillMaxWidth().drawBehind { tiles.forEach { drawTile(it, color) } },
    ) { measurables, constraints ->
        val gap = ThemeSpace.line.roundToPx()
        val placeables = measurables.map { it.measure(constraints.copy(minHeight = 0)) }
        val shown = placeables.indices.filter { placeables[it].height > 0 }
        val stacked = stack(shown.map { placeables[it].height to (measurables[it].parentData is JoinsTileAbove) }, gap)
        if (stacked.tiles != tiles) tiles = stacked.tiles
        layout(constraints.maxWidth, stacked.height) {
            shown.forEachIndexed { n, index ->
                val shape = tileShape(top = n == 0, bottom = n == shown.lastIndex)
                placeables[index].placeWithLayer(0, stacked.tops[n]) {
                    this.shape = shape
                    clip = true
                }
            }
        }
    }
}

private fun DrawScope.drawTile(
    tile: Tile,
    color: Color,
) {
    val outer = ThemeRadius.panel.toPx()
    val inner = INNER_RADIUS.toPx()
    val top = CornerRadius(if (tile.first) outer else inner)
    val bottom = CornerRadius(if (tile.last) outer else inner)
    val rect =
        RoundRect(
            left = 0f,
            top = tile.top.toFloat(),
            right = size.width,
            bottom = (tile.top + tile.height).toFloat(),
            topLeftCornerRadius = top,
            topRightCornerRadius = top,
            bottomRightCornerRadius = bottom,
            bottomLeftCornerRadius = bottom,
        )
    drawPath(Path().apply { addRoundRect(rect) }, color)
}

/** Where the children sit, top to bottom, the tiles behind them and the total height. */
private class Stacked(
    val tops: List<Int>,
    val tiles: List<Tile>,
    val height: Int,
)

/** Stacks children of the given heights [gap] apart, a child that joins the tile above it with none. */
private fun stack(
    children: List<Pair<Int, Boolean>>,
    gap: Int,
): Stacked {
    val tiles = mutableListOf<Tile>()
    val tops = mutableListOf<Int>()
    var y = 0
    children.forEachIndexed { n, (height, joins) ->
        val joined = joins && tiles.isNotEmpty()
        if (n > 0 && !joined) y += gap
        tops += y
        if (joined) {
            tiles[tiles.lastIndex] = tiles.last().copy(height = y + height - tiles.last().top)
        } else {
            tiles += Tile(top = y, height = height, first = n == 0, last = false)
        }
        y += height
    }
    if (tiles.isNotEmpty()) tiles[tiles.lastIndex] = tiles.last().copy(last = true)
    return Stacked(tops, tiles, y)
}

/** One tile of the card: where it sits and whether it is the group's first or last. */
private data class Tile(
    val top: Int,
    val height: Int,
    val first: Boolean,
    val last: Boolean,
)

/** The corners of a child at the group's [top] or [bottom], rounded as the tile behind it is. */
private fun tileShape(
    top: Boolean,
    bottom: Boolean,
): RoundedCornerShape {
    val outer = ThemeRadius.panel
    val top = if (top) outer else INNER_RADIUS
    val bottom = if (bottom) outer else INNER_RADIUS
    return RoundedCornerShape(topStart = top, topEnd = top, bottomEnd = bottom, bottomStart = bottom)
}

private object JoinsTileAbove : ParentDataModifier {
    override fun Density.modifyParentData(parentData: Any?): Any = JoinsTileAbove
}

/** Makes this child of an [AndroidCard] share the tile of the child above it, with no gap between them. */
internal fun Modifier.joinsTileAbove(): Modifier = this.then(JoinsTileAbove)

/** The corner between two tiles. */
private val INNER_RADIUS = 4.dp
