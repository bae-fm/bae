package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.number
import fm.bae.app.ratersText
import fm.bae.app.starsAfterTapping
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidDivider
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSheetHeading
import fm.bae.app.ui.components.Stars
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeStars

private val STAR_ROW_SIZE = 18.dp
private val ROW_HEIGHT = 48.dp

/** The sheet that rates the album: tapping a row rates it with that many stars and closes the sheet. */
@Composable
internal fun AlbumRatingSheet(
    rating: BridgeAlbumRating,
    onRate: (BridgeStars?) -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidSheet(onDismiss = onDismiss, heading = AndroidSheetHeading(title = stringResource(R.string.rate))) {
        AlbumRatingChoices(rating, onRate = {
            onRate(it)
            onDismiss()
        })
    }
}

/**
 * What the rating sheet holds under its title: who rated the album, five stars down to one with the person's own
 * checked, then Clear rating. A shot draws it in a sheet's place, because the sheet opens a window.
 */
@Composable
internal fun ColumnScope.AlbumRatingChoices(
    rating: BridgeAlbumRating,
    onRate: (BridgeStars?) -> Unit,
) {
    Text(
        text = LocalContext.current.ratersText(rating),
        style = ThemeText.body.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.related),
    )
    BridgeStars.entries.reversed().forEach { stars ->
        RatingRow(stars = stars, checked = stars == rating.mine, onClick = { onRate(rating.starsAfterTapping(stars)) })
    }
    AndroidDivider()
    val rated = rating.mine != null
    Box(
        modifier =
            Modifier
                .fillMaxWidth()
                .defaultMinSize(minHeight = ROW_HEIGHT)
                .clickable(enabled = rated, onClick = { onRate(null) })
                .padding(horizontal = ThemeSpace.edge),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(
            text = stringResource(R.string.clear_rating),
            style = ThemeText.body.style,
            color = if (rated) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** One choice: its stars, with a check at the end when it is the person's own rating. */
@Composable
private fun RatingRow(
    stars: BridgeStars,
    checked: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .defaultMinSize(minHeight = ROW_HEIGHT)
                .clickable(onClick = onClick)
                .padding(horizontal = ThemeSpace.edge)
                .semantics(mergeDescendants = true) { selected = checked },
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Stars(value = stars.number, size = STAR_ROW_SIZE)
        if (checked) {
            Icon(
                imageVector = SymbolIcons.Check,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(ThemeIcon.medium),
            )
        }
    }
}
