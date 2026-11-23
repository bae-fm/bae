package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.Artwork
import fm.bae.app.ui.components.IconTone
import uniffi.bae_bridge.BridgeImageRef

/**
 * One track of the queue: its cover, [title], [artist] (null for a track the library no longer holds), [album] and
 * [duration], with Remove and the drag handle ([dragHandleModifier] makes it draggable); tapping the row calls
 * [onClick]. The [current] track is the one playing: its title in the accent colour with its artist only, and no
 * controls.
 */
@Composable
internal fun AndroidQueueRow(
    title: String,
    artist: String?,
    cover: BridgeImageRef?,
    album: String? = null,
    duration: String? = null,
    current: Boolean = false,
    dragHandleModifier: Modifier = Modifier,
    onClick: () -> Unit = {},
    onRemove: () -> Unit = {},
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (current) Modifier else Modifier.clickable(onClick = onClick))
                .padding(
                    start = ThemeSpace.group,
                    top = ThemeSpace.related,
                    end = ThemeSpace.inline,
                    bottom = ThemeSpace.related,
                ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Artwork(
            cover = cover,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = ThemeText.rowTitle.style,
                color = if (current) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
            )
            if (artist != null) DetailLine(artist)
            if (!current && album != null) DetailLine(album)
        }
        if (!current) {
            if (duration != null) {
                Text(
                    text = duration,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    // The board puts a group's space between a row's pieces and a second one beside the Remove button.
                    modifier = Modifier.padding(end = ThemeSpace.group * 2),
                )
            }
            AndroidIconButton(
                icon = SymbolIcons.Close,
                contentDescription = stringResource(R.string.queue_remove),
                onClick = onRemove,
                tone = IconTone.MUTED,
            )
            Spacer(modifier = Modifier.width(ThemeSpace.group))
            Icon(
                imageVector = SymbolIcons.DragHandle,
                contentDescription = stringResource(R.string.queue_drag_to_reorder),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = dragHandleModifier.size(ThemeIcon.medium),
            )
        }
    }
}

@Composable
private fun DetailLine(text: String) {
    Text(
        text = text,
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
    )
}
