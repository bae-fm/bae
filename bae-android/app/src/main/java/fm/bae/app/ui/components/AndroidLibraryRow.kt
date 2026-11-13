package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgePlaylistCover

/**
 * One row of a library list: the picture [art], the [title], a second line [subtitle] (left out when
 * null or blank, so the row is shorter) and a [trailing] label at the end, a track's duration. A track
 * or a credit has no picture ([noArt]); an artist or composer with none is a plain tile ([missing]).
 * A playlist draws its [playlistCover] where the others draw [art]. A [part] is indented under its
 * group's heading work; an [inset] row, on the composer and work pages, sets its text further in,
 * in a taller row. A row with an [onClick] opens something.
 */
@Composable
fun AndroidLibraryRow(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    trailing: String? = null,
    art: BridgeImageRef? = null,
    playlistCover: BridgePlaylistCover? = null,
    noArt: Boolean = false,
    missing: Boolean = false,
    part: Boolean = false,
    inset: Boolean = false,
    onClick: (() -> Unit)? = null,
) {
    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
                .padding(start = if (part) ThemeSpace.edge else 0.dp)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!noArt) {
            RowArt(art = art, playlistCover = playlistCover, missing = missing, description = title)
            Spacer(modifier = Modifier.width(ThemeSpace.group))
        }
        Column(
            modifier =
                Modifier
                    .weight(1f)
                    .then(
                        if (inset) {
                            Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related)
                        } else {
                            Modifier
                        },
                    ),
        ) {
            Text(
                text = title,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (!subtitle.isNullOrBlank()) {
                Text(
                    text = subtitle,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (trailing != null) {
            Spacer(modifier = Modifier.width(ThemeSpace.group))
            Text(
                text = trailing,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** The row's picture, or a plain tile where there is none. */
@Composable
private fun RowArt(
    art: BridgeImageRef?,
    playlistCover: BridgePlaylistCover?,
    missing: Boolean,
    description: String,
) {
    val tile = Modifier.size(ThemeSize.rowArtwork)
    if (playlistCover != null) {
        PlaylistPicture(
            cover = playlistCover,
            cornerRadius = ThemeRadius.artwork,
            modifier = tile,
            contentDescription = description,
        )
    } else if (missing) {
        Box(tile.clip(RoundedCornerShape(ThemeRadius.artwork)).background(BaeTheme.surfaces.placeholder))
    } else {
        CoverImage(
            cover = art,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = tile,
            contentDescription = description,
        )
    }
}
