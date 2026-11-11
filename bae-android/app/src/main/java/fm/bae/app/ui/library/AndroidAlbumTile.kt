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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import uniffi.bae_bridge.BridgeImageRef

/** An album's tile: its [cover], [title] and a [line] under it, opening the album. */
@Composable
internal fun AndroidAlbumTile(
    title: String,
    line: String,
    cover: BridgeImageRef?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.clickable(onClick = onClick)) {
        CoverImage(
            cover = cover,
            cornerRadius = ThemeRadius.cover,
            iconPadding = 40.dp,
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
            contentDescription = title,
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
