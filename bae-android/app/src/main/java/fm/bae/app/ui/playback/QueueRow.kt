package fm.bae.app.ui.playback

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.durationClockLabel
import fm.bae.app.playback.NowPlaying
import fm.bae.app.playback.QueueItem
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import uniffi.bae_bridge.BridgeDurationClock

// The queue's row renderers; QueueScreen.kt lays them out.

@Composable
internal fun NowPlayingRow(np: NowPlaying) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = np.coverImage,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = np.title,
                style = ThemeText.rowTitle.style,
                color = MaterialTheme.colorScheme.primary,
                maxLines = 1,
            )
            Text(
                text = np.artist,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
internal fun QueueRow(
    item: QueueItem,
    dragHandleModifier: Modifier,
    onClick: () -> Unit,
    onRemove: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = item.coverImage,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        QueueItemText(item, modifier = Modifier.weight(1f))
        // Hidden rather than removed when there is no duration, so rows align.
        val durationLabel = LocalContext.current.durationClockLabel(item.durationClock)
        Text(
            text = durationLabel,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier =
                Modifier
                    .padding(horizontal = ThemeSpace.related)
                    .alpha(if (durationLabel.isEmpty()) 0f else 1f),
        )
        IconButton(onClick = onRemove) {
            Icon(
                imageVector = Icons.Filled.Close,
                contentDescription = stringResource(R.string.queue_remove),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Icon(
            imageVector = Icons.Filled.DragHandle,
            contentDescription = stringResource(R.string.queue_drag_to_reorder),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = dragHandleModifier.size(ThemeIcon.medium),
        )
    }
}

/** A skeleton for a row whose item hasn't loaded yet. */
@Composable
internal fun QueueRowPlaceholder() {
    val placeholderColor = BaeTheme.surfaces.placeholder
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier =
                Modifier
                    .size(ThemeSize.rowArtwork)
                    .clip(RoundedCornerShape(ThemeRadius.artwork))
                    .background(placeholderColor),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Box(
                modifier =
                    Modifier
                        .size(width = 160.dp, height = 12.dp)
                        .clip(RoundedCornerShape(ThemeRadius.bar))
                        .background(placeholderColor),
            )
            Spacer(modifier = Modifier.height(ThemeSpace.compact))
            Box(
                modifier =
                    Modifier
                        .size(width = 100.dp, height = 10.dp)
                        .clip(RoundedCornerShape(ThemeRadius.bar))
                        .background(placeholderColor),
            )
            Spacer(modifier = Modifier.height(ThemeSpace.compact))
            Box(
                modifier =
                    Modifier
                        .size(width = 120.dp, height = 10.dp)
                        .clip(RoundedCornerShape(ThemeRadius.bar))
                        .background(placeholderColor),
            )
        }
    }
}

@Composable
private fun QueueItemText(
    item: QueueItem,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
        Text(
            text = item.title,
            style = ThemeText.rowTitle.style,
            maxLines = 1,
        )
        Text(
            text = item.artist,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
        Text(
            text = item.albumTitle,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
}

private val previewNowPlaying =
    NowPlaying(
        trackId = "trk-1",
        title = "Track Title",
        artist = "Artist Name",
        coverImage = PreviewData.imageRef("rel-1"),
        sidePausePrompt = null,
    )

private val previewQueueItem =
    QueueItem(
        entryId = "entry-1",
        trackId = "trk-1",
        title = "Track Title",
        artist = "Artist Name",
        albumTitle = "Album Title",
        // Built directly because previews can't call the native bridge.
        durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 34u),
        coverImage = PreviewData.imageRef("rel-1"),
    )

@Preview(showBackground = true)
@Composable
private fun NowPlayingRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            NowPlayingRow(np = previewNowPlaying)
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun QueueRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            QueueRow(
                item = previewQueueItem,
                dragHandleModifier = Modifier,
                onClick = {},
                onRemove = {},
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun QueueRowPlaceholderPreview() {
    BaeTheme {
        QueueRowPlaceholder()
    }
}
