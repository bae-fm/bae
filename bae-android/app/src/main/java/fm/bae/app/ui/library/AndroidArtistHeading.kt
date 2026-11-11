package fm.bae.app.ui.library

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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.layout.layout
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import uniffi.bae_bridge.BridgeLibraryBrowseSection

/** The artist's picture in the slim header pinned at the top of the grid. */
private val PINNED_PICTURE = 32.dp

/**
 * An artist's group in the album grid grouped by artist: the artist's round
 * picture, name and album count, opening the artist. Pinned at the top while
 * its albums scroll by, it shows as a slim strip of picture and name across
 * the grid's width, [gridPadding] past each side of its item. The item keeps
 * the full header's height either way, so the albums under it don't move as
 * it pins; the space below the strip is left empty, and touches there reach
 * the albums beneath.
 */
@Composable
internal fun AndroidArtistHeading(
    section: BridgeLibraryBrowseSection,
    pinned: Boolean,
    gridPadding: Dp,
    onClick: () -> Unit,
) {
    Box(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .then(
                        // Pinned, it only holds the item's height: unseen, untouchable and unread.
                        if (pinned) {
                            Modifier.alpha(0f).clearAndSetSemantics {}
                        } else {
                            Modifier.clickable(onClick = onClick).semantics { heading() }
                        },
                    ).padding(top = ThemeSpace.inline),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ArtistPicture(section = section, size = ThemeSize.rowArtwork)
            Spacer(modifier = Modifier.width(ThemeSpace.group))
            Column(modifier = Modifier.weight(1f)) {
                ArtistName(section)
                val albums = section.window.limit
                Text(
                    text = pluralStringResource(R.plurals.album_count, albums.toInt(), albums.toLong()),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (pinned) {
            Column(
                modifier =
                    Modifier
                        .bleed(gridPadding)
                        .background(MaterialTheme.colorScheme.background)
                        .clickable(onClick = onClick)
                        .semantics { heading() },
            ) {
                Row(
                    modifier =
                        Modifier.padding(horizontal = gridPadding, vertical = ThemeSpace.related),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    ArtistPicture(section = section, size = PINNED_PICTURE)
                    Spacer(modifier = Modifier.width(ThemeSpace.group))
                    ArtistName(section)
                }
                HorizontalDivider(thickness = ThemeSpace.hairline)
            }
        }
    }
}

@Composable
private fun ArtistPicture(
    section: BridgeLibraryBrowseSection,
    size: Dp,
) {
    CoverImage(
        cover = section.image,
        cornerRadius = size / 2,
        iconPadding = ThemeSpace.related,
        modifier = Modifier.size(size),
    )
}

@Composable
private fun ArtistName(section: BridgeLibraryBrowseSection) {
    Text(
        text = section.title,
        style = ThemeText.heading.style,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/** Widens the content by [by] past each side of the space it's given. */
private fun Modifier.bleed(by: Dp): Modifier =
    layout { measurable, constraints ->
        val extra = by.roundToPx()
        val width = constraints.maxWidth + 2 * extra
        val placeable = measurable.measure(constraints.copy(minWidth = width, maxWidth = width))
        layout(constraints.maxWidth, placeable.height) { placeable.place(-extra, 0) }
    }
