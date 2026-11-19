package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import fm.bae.app.number
import fm.bae.app.ratersText
import fm.bae.app.starsAfterTapping
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Stars
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeStars

/** Shows the album's stars, which the person taps to rate it, and the line that names who rated it. */
@Composable
internal fun AlbumRating(
    rating: BridgeAlbumRating,
    onRate: (BridgeStars?) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(ThemeSpace.line)) {
        Stars(
            value = rating.mine?.number ?: 0,
            size = 24.dp,
            onRate = { onRate(rating.starsAfterTapping(it)) },
        )
        Text(
            text = context.ratersText(rating),
            style = ThemeText.fine.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
