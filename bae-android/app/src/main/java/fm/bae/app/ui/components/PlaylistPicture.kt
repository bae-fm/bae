package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import uniffi.bae_bridge.BridgePlaylistCover

/** The gap between the four covers of a grid. */
private val GRID_GAP = 1.dp

/**
 * A playlist's picture, sized by [modifier]: one cover filling the square, or four covers as a 2×2
 * square. A missing cover is the plain tile in its place.
 */
@Composable
fun PlaylistPicture(
    cover: BridgePlaylistCover,
    cornerRadius: Dp,
    modifier: Modifier = Modifier,
    contentDescription: String? = null,
) {
    when (cover) {
        is BridgePlaylistCover.Single -> {
            CoverImage(
                cover = cover.cover,
                cornerRadius = cornerRadius,
                iconPadding = ThemeSpace.group,
                modifier = modifier,
                contentDescription = contentDescription,
            )
        }

        is BridgePlaylistCover.Grid -> {
            Column(
                modifier = modifier.clip(RoundedCornerShape(cornerRadius)),
                verticalArrangement = Arrangement.spacedBy(GRID_GAP),
            ) {
                GridRow(cover, from = 0)
                GridRow(cover, from = 2)
            }
        }
    }
}

@Composable
private fun ColumnScope.GridRow(
    cover: BridgePlaylistCover.Grid,
    from: Int,
) {
    Row(
        modifier = Modifier.weight(1f).fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(GRID_GAP),
    ) {
        for (index in from until from + 2) {
            CoverImage(
                cover = cover.covers.getOrNull(index),
                cornerRadius = 0.dp,
                iconPadding = 0.dp,
                modifier = Modifier.weight(1f).fillMaxSize(),
            )
        }
    }
}
