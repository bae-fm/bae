package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.copyLineText
import fm.bae.app.creditLineText
import fm.bae.app.editionLineText
import fm.bae.app.factSeparator
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.Artwork
import fm.bae.app.ui.components.IconTone
import fm.bae.app.ui.components.Stars
import fm.bae.app.yearText
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeStars

private val COVER_SIZE = 150.dp
private val STAR_SIZE = 12.dp

/**
 * The album page's head: the cover, then the title, the artist, the year with the person's rating after it, the
 * release's edition and copy lines. Under them, [actions] supplies Play, or [onShowWhere] opens the device
 * sheet for a release held elsewhere.
 */
@Composable
internal fun AndroidAlbumHeader(
    detail: BridgeAlbumDetail,
    cover: BridgeImageRef?,
    galleryItemCount: UInt,
    onShowGallery: () -> Unit,
    onShowWhere: () -> Unit,
    actions: @Composable () -> Unit = {},
) {
    val album = detail.album
    val context = LocalContext.current

    Row(modifier = Modifier.height(IntrinsicSize.Min), verticalAlignment = Alignment.Top) {
        Artwork(
            cover = cover,
            cornerRadius = ThemeRadius.cover,
            iconPadding = ThemeSpace.page,
            modifier =
                Modifier
                    .size(COVER_SIZE)
                    .clickable(enabled = galleryItemCount > 0u, onClick = onShowGallery),
            contentDescription = album.title,
        )
        Column(modifier = Modifier.weight(1f).fillMaxHeight().padding(start = ThemeSpace.edge)) {
            Text(text = album.title, style = ThemeText.hero.style)
            Text(
                text = context.creditLineText(album.artistLine),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            YearAndRating(year = album.year?.let(context::yearText), rating = detail.rating.mine)
            val release = detail.shownRelease
            release.editionLineText(context).takeIf { it.isNotEmpty() }?.let {
                Spacer(modifier = Modifier.height(ThemeSpace.line))
                ReleaseLine(it)
            }
            release.copyLineText(context).takeIf { it.isNotEmpty() }?.let { ReleaseLine(it) }
            Spacer(modifier = Modifier.weight(1f))
            Box(modifier = Modifier.padding(top = ThemeSpace.compact)) {
                if (release.playability is BridgeReleasePlayability.OnOtherDevice) {
                    NotOnThisDevice(onShowWhere)
                } else {
                    actions()
                }
            }
        }
    }
}

/** One of the release's lines, in the tertiary colour, on one line. */
@Composable
private fun ReleaseLine(text: String) {
    Text(
        text = text,
        style = ThemeText.detail.style,
        color = BaeTheme.colors.textTertiary,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/** The album's [year], then the rating the person gave it, as its number and one star, when it has one. */
@Composable
private fun YearAndRating(
    year: String?,
    rating: BridgeStars?,
) {
    if (year == null && rating == null) return
    val color = MaterialTheme.colorScheme.onSurfaceVariant
    Row(verticalAlignment = Alignment.CenterVertically) {
        year?.let { Text(text = it, style = ThemeText.detail.style, color = color) }
        if (rating != null) {
            if (year != null) {
                Text(text = LocalContext.current.factSeparator(), style = ThemeText.detail.style, color = color)
            }
            Stars(rating = rating, size = STAR_SIZE)
        }
    }
}

/** The words for a release only another device holds, where Play would stand, and the button that says where it is. */
@Composable
private fun NotOnThisDevice(onShowWhere: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            text = stringResource(R.string.release_not_on_this_device),
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        AndroidIconButton(
            icon = SymbolIcons.Info,
            contentDescription = stringResource(R.string.release_show_where),
            onClick = onShowWhere,
            tone = IconTone.MUTED,
            glyph = ThemeIcon.medium,
        )
    }
}
