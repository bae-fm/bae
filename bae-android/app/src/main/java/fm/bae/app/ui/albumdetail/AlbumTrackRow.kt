package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/** The position column's least width, which the now-playing icon in the
 *  number's place also fits. */
private val positionColumnMinWidth = 40.dp

/** The position column's width: the widest of a release's [positionLabels]
 *  in the label's style plus the gap before the title, so no label wraps and
 *  every row's title starts at the same x. */
@Composable
internal fun rememberPositionColumnWidth(positionLabels: List<String>): Dp {
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    return remember(positionLabels, measurer, density) {
        val widest =
            positionLabels.maxOfOrNull { label ->
                measurer.measure(label, style = ThemeText.body.style, maxLines = 1, softWrap = false).size.width
            } ?: 0
        maxOf(positionColumnMinWidth, with(density) { widest.toDp() } + ThemeSpace.related)
    }
}

internal data class TrackRowData(
    val positionLabel: String,
    val title: String,
    val artistNames: String?,
    val durationLabel: String,
    val isCurrent: Boolean,
    val isPlaying: Boolean,
    /** The track's audio is still arriving from another device: it shows dimmed,
     *  says so in place of its length, and can't play yet. */
    val isArriving: Boolean,
)

internal data class TrackRowCallbacks(
    val onClick: () -> Unit,
    val onPlayNext: () -> Unit,
    val onAddToQueue: () -> Unit,
)

@Composable
internal fun TrackRow(
    data: TrackRowData,
    callbacks: TrackRowCallbacks,
    positionColumnWidth: Dp,
) {
    var menuExpanded by remember { mutableStateOf(false) }
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .alpha(if (data.isArriving) ThemeOpacity.muted.toFloat() else 1f)
                .clickable(enabled = !data.isArriving, onClick = callbacks.onClick)
                .padding(vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TrackPositionIndicator(data.isCurrent, data.isPlaying, data.positionLabel, positionColumnWidth)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = data.title,
                style = ThemeText.rowTitle.style,
                color =
                    if (data.isCurrent) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.onSurface
                    },
                maxLines = 1,
            )
            if (data.artistNames != null) {
                Text(
                    text = data.artistNames,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
        if (data.isArriving) {
            Text(
                text = LocalContext.current.coreString("core.arrival.track"),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else if (data.durationLabel.isNotEmpty()) {
            Text(
                text = data.durationLabel,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        TrackRowMenu(
            enabled = !data.isArriving,
            menuExpanded = menuExpanded,
            onExpand = { menuExpanded = true },
            onDismiss = { menuExpanded = false },
            onPlayNext = callbacks.onPlayNext,
            onAddToQueue = callbacks.onAddToQueue,
        )
    }
}

@Composable
private fun TrackPositionIndicator(
    isCurrent: Boolean,
    isPlaying: Boolean,
    positionLabel: String,
    width: Dp,
) {
    if (isCurrent) {
        Box(modifier = Modifier.width(width), contentAlignment = Alignment.CenterStart) {
            Icon(
                imageVector =
                    if (isPlaying) Icons.AutoMirrored.Filled.VolumeUp else Icons.Filled.PlayArrow,
                contentDescription =
                    if (isPlaying) stringResource(R.string.now_playing) else stringResource(R.string.paused),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    } else {
        Text(
            text = positionLabel,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            softWrap = false,
            modifier = Modifier.width(width),
        )
    }
}

@Composable
private fun TrackRowMenu(
    enabled: Boolean,
    menuExpanded: Boolean,
    onExpand: () -> Unit,
    onDismiss: () -> Unit,
    onPlayNext: () -> Unit,
    onAddToQueue: () -> Unit,
) {
    Box {
        IconButton(onClick = onExpand, enabled = enabled) {
            Icon(
                imageVector = Icons.Filled.MoreVert,
                contentDescription = stringResource(R.string.track_options),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        DropdownMenu(expanded = menuExpanded, onDismissRequest = onDismiss) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.play_next)) },
                onClick = {
                    onDismiss()
                    onPlayNext()
                },
            )
            DropdownMenuItem(
                text = { Text(stringResource(R.string.add_to_queue)) },
                onClick = {
                    onDismiss()
                    onAddToQueue()
                },
            )
        }
    }
}

private val previewTrackRowCallbacks = TrackRowCallbacks(onClick = {}, onPlayNext = {}, onAddToQueue = {})

/** The previewed rows' positions, as one release's. */
private val previewPositionLabels = listOf("1", "2", "3")

@Preview(showBackground = true)
@Composable
private fun TrackRowPreview() {
    BaeTheme {
        TrackRow(
            data =
                TrackRowData(
                    positionLabel = "1",
                    title = "Track Title",
                    artistNames = "Artist Name",
                    durationLabel = "3:34",
                    isCurrent = false,
                    isPlaying = false,
                    isArriving = false,
                ),
            callbacks = previewTrackRowCallbacks,
            positionColumnWidth = rememberPositionColumnWidth(previewPositionLabels),
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun TrackRowCurrentPreview() {
    BaeTheme {
        TrackRow(
            data =
                TrackRowData(
                    positionLabel = "2",
                    title = "Track Title",
                    artistNames = null,
                    durationLabel = "4:12",
                    isCurrent = true,
                    isPlaying = true,
                    isArriving = false,
                ),
            callbacks = previewTrackRowCallbacks,
            positionColumnWidth = rememberPositionColumnWidth(previewPositionLabels),
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun TrackRowArrivingPreview() {
    BaeTheme {
        TrackRow(
            data =
                TrackRowData(
                    positionLabel = "3",
                    title = "Track Title",
                    artistNames = null,
                    durationLabel = "5:02",
                    isCurrent = false,
                    isPlaying = false,
                    isArriving = true,
                ),
            callbacks = previewTrackRowCallbacks,
            positionColumnWidth = rememberPositionColumnWidth(previewPositionLabels),
        )
    }
}
