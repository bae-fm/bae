package fm.bae.app.ui

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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R

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
)

@Composable
internal fun TrackRow(
    data: TrackRowData,
    callbacks: TrackRowCallbacks,
) {
    var menuExpanded by remember { mutableStateOf(false) }
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = callbacks.onClick)
                .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TrackPositionIndicator(data.isCurrent, data.isPlaying, data.positionLabel)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = data.title,
                style = MaterialTheme.typography.bodyMedium,
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
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
        if (data.durationLabel.isNotEmpty()) {
            Text(
                text = data.durationLabel,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        TrackRowMenu(
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
) {
    if (isCurrent) {
        Box(modifier = Modifier.width(40.dp), contentAlignment = Alignment.CenterStart) {
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
            text = positionLabel.ifEmpty { "-" },
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.width(40.dp),
        )
    }
}

@Composable
private fun TrackRowMenu(
    menuExpanded: Boolean,
    onExpand: () -> Unit,
    onDismiss: () -> Unit,
    onPlayNext: () -> Unit,
    onAddToQueue: () -> Unit,
) {
    Box {
        IconButton(onClick = onExpand) {
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
