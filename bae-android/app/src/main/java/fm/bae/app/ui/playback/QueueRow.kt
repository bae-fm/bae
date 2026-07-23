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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.durationClockLabel
import fm.bae.app.playback.NowPlaying
import fm.bae.app.playback.QueueItem
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.CoverBytesCache
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.LocalCoverBytesCache
import uniffi.bae_bridge.BridgeDurationClock

// The queue's row renderers — the current-track row, a loaded queue row, the
// not-yet-loaded skeleton, and the shared title/artist block. Kept beside the
// screen scaffolding in QueueScreen.kt, which addresses and lays them out.

@Composable
internal fun NowPlayingRow(
    np: NowPlaying,
    loadImage: suspend (imageId: String) -> ByteArray?,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            coverId = np.coverImageId,
            coverVersion = null,
            loadImage = loadImage,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = np.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.primary,
                maxLines = 1,
            )
            Text(
                text = np.artist,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
internal fun QueueRow(
    item: QueueItem,
    loadImage: suspend (imageId: String) -> ByteArray?,
    dragHandleModifier: Modifier,
    onClick: () -> Unit,
    onRemove: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            coverId = item.coverImageId,
            coverVersion = null,
            loadImage = loadImage,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
        )
        Spacer(modifier = Modifier.width(12.dp))
        QueueItemText(item, modifier = Modifier.weight(1f))
        // The label is empty when core reports no duration; keep the slot in the
        // tree and toggle via alpha so rows align.
        val durationLabel = LocalContext.current.durationClockLabel(item.durationClock)
        Text(
            text = durationLabel,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier =
                Modifier
                    .padding(horizontal = 8.dp)
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
            modifier = dragHandleModifier.size(24.dp),
        )
    }
}

/** A not-yet-loaded row: a skeleton shape, no text — `loadRange` is already in
 *  flight for it via the row's `LaunchedEffect`. */
@Composable
internal fun QueueRowPlaceholder() {
    val placeholderColor = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.15f)
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier =
                Modifier
                    .size(48.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(placeholderColor),
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Box(
                modifier =
                    Modifier
                        .size(width = 160.dp, height = 12.dp)
                        .clip(RoundedCornerShape(3.dp))
                        .background(placeholderColor),
            )
            Spacer(modifier = Modifier.height(6.dp))
            Box(
                modifier =
                    Modifier
                        .size(width = 100.dp, height = 10.dp)
                        .clip(RoundedCornerShape(3.dp))
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
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
        )
        Text(
            text =
                if (item.albumTitle.isEmpty()) {
                    item.artist
                } else {
                    "${item.artist} — ${item.albumTitle}"
                },
            style = MaterialTheme.typography.bodySmall,
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
        coverImageId = "rel-1",
        sidePausePrompt = null,
    )

private val previewQueueItem =
    QueueItem(
        entryId = "entry-1",
        trackId = "trk-1",
        title = "Track Title",
        artist = "Artist Name",
        albumTitle = "Album Title",
        // Built in-process (never the `bridgeClock` FFI) so the @Preview renders
        // under layoutlib, which can't call into the native bridge.
        durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 34u),
        coverImageId = "rel-1",
    )

@Preview(showBackground = true)
@Composable
private fun NowPlayingRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalCoverBytesCache provides CoverBytesCache()) {
            NowPlayingRow(np = previewNowPlaying, loadImage = { _ -> null })
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun QueueRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalCoverBytesCache provides CoverBytesCache()) {
            QueueRow(
                item = previewQueueItem,
                loadImage = { _ -> null },
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
