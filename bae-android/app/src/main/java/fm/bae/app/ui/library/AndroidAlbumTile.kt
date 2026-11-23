package fm.bae.app.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Artwork
import uniffi.bae_bridge.BridgeImageRef

/** An album's tile opens its page and dims its cover and words when [unplayable]. */
@Composable
internal fun AndroidAlbumTile(
    title: String,
    line: String,
    cover: BridgeImageRef?,
    unplayable: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val unavailable = if (unplayable) stringResource(R.string.release_not_on_this_device) else null
    Column(
        modifier =
            modifier
                .clickable(role = Role.Button, onClick = onClick)
                .semantics { unavailable?.let { stateDescription = it } }
                .alpha(if (unplayable) ThemeOpacity.dimmed else 1f),
    ) {
        Artwork(
            cover = cover,
            cornerRadius = ThemeRadius.cover,
            iconPadding = 40.dp,
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
        )
        Spacer(modifier = Modifier.height(ThemeSpace.compact))
        Text(
            text = title,
            style = ThemeText.rowTitle.style,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = line,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
