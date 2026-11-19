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
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.factsLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.TrackOptionsMenu
import uniffi.bae_bridge.BridgeTrackWork

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
)

internal data class TrackRowCallbacks(
    val onClick: () -> Unit,
    val onPlayNext: () -> Unit,
    val onAddToQueue: () -> Unit,
    /** The menu shows Add to playlist only when the person can make playlists; null hides it. */
    val onAddToPlaylist: (() -> Unit)? = null,
)

/**
 * A track's row: its lead, title and artists, length and ⋮, then a line for each of the [works] it
 * performs, lined up with the title, which [onSelectWork] opens.
 */
@Composable
internal fun AndroidTrackRow(
    data: TrackRowData,
    callbacks: TrackRowCallbacks,
    positionColumnWidth: Dp,
    works: List<BridgeTrackWork> = emptyList(),
    onSelectWork: (String) -> Unit = {},
) {
    Column {
        TrackLine(data, callbacks, positionColumnWidth)
        works.forEach { work -> TrackWorkLine(work, positionColumnWidth, onSelectWork) }
    }
}

/** The row's line: lead, title and artists, length and ⋮. */
@Composable
private fun TrackLine(
    data: TrackRowData,
    callbacks: TrackRowCallbacks,
    positionColumnWidth: Dp,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = callbacks.onClick)
                .padding(vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AndroidTrackLead(data.isCurrent, data.isPlaying, data.positionLabel, positionColumnWidth)
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
        Text(
            text = data.durationLabel,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = ThemeSpace.related),
        )
        TrackOptionsMenu(
            onPlayNext = callbacks.onPlayNext,
            onAddToQueue = callbacks.onAddToQueue,
            onAddToPlaylist = callbacks.onAddToPlaylist,
        )
    }
}

/** A track row's lead: its number, or the accent speaker or play glyph on the current track. */
@Composable
internal fun AndroidTrackLead(
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

/**
 * Shows a work the track performs under the track's row, lined up with the title: the work's title in the
 * accent colour, then its composers. A tap opens the work.
 */
@Composable
private fun TrackWorkLine(
    work: BridgeTrackWork,
    positionColumnWidth: Dp,
    onSelectWork: (String) -> Unit,
) {
    val text = LocalContext.current.factsLine(listOf(work.title, work.composerNames))
    Text(
        text = text,
        style = ThemeText.fine.style,
        color = MaterialTheme.colorScheme.primary,
        modifier =
            Modifier
                .clickable { onSelectWork(work.workId) }
                .padding(start = positionColumnWidth, bottom = ThemeSpace.compact),
    )
}
