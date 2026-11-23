package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.number
import fm.bae.app.ratersText
import fm.bae.app.starsAfterTapping
import fm.bae.app.ui.components.AndroidDivider
import fm.bae.app.ui.components.AndroidMenuItemStyle
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSheetHeading
import fm.bae.app.ui.components.Stars
import fm.bae.app.ui.library.AndroidMenuItem
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeStars

private val STAR_ROW_SIZE = 18.dp

/** The sheet that rates the album: tapping a row rates it with that many stars and closes the sheet. */
@Composable
internal fun AlbumRatingSheet(
    rating: BridgeAlbumRating.Stars,
    onRate: (BridgeStars?) -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidSheet(onDismiss = onDismiss, heading = albumRatingHeading(rating)) {
        AlbumRatingChoices(rating, onRate = {
            onRate(it)
            onDismiss()
        })
    }
}

/** The rating sheet's title, with who rated the album as its note when anybody did. */
@Composable
internal fun albumRatingHeading(rating: BridgeAlbumRating.Stars): AndroidSheetHeading =
    AndroidSheetHeading(
        title = stringResource(R.string.rate),
        note = rating.raters?.let { LocalContext.current.ratersText(it) },
    )

/**
 * What the rating sheet holds under its heading: five stars down to one with the person's own checked, then
 * Clear rating. A shot draws it in a sheet's place, because the sheet opens a window.
 */
@Composable
internal fun ColumnScope.AlbumRatingChoices(
    rating: BridgeAlbumRating.Stars,
    onRate: (BridgeStars?) -> Unit,
) {
    AndroidMenuItemStyle {
        BridgeStars.entries.reversed().forEach { stars ->
            AndroidMenuItem(
                label = stringResource(R.string.stars_of_five, stars.number),
                selected = false,
                check = stars == rating.mine,
                onClick = { onRate(rating.starsAfterTapping(stars)) },
                content = { Stars(value = stars.number, size = STAR_ROW_SIZE) },
            )
        }
        AndroidDivider()
        AndroidMenuItem(
            label = stringResource(R.string.clear_rating),
            selected = false,
            enabled = rating.mine != null,
            onClick = { onRate(null) },
        )
    }
}
