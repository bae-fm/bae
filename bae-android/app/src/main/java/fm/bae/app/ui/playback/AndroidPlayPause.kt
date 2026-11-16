package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.R

/** How far the arc sits outside the glyph, on each side. */
private val arcClearance = 8.dp

/**
 * Play/pause toggle, its glyph [iconSize], which differs between the bar and
 * the expanded player. While core loads a track it shows pause, and a press
 * makes the track start paused when it finishes loading. While
 * [waitsOnStorage] is true, an arc turns around it.
 */
@Composable
fun AndroidPlayPause(
    isPlaying: Boolean,
    iconSize: Dp,
    waitsOnStorage: Boolean,
    onToggle: () -> Unit,
) {
    val accent = MaterialTheme.colorScheme.primary.toArgb()
    val waiting = stringResource(R.string.downloading_the_track)
    Box(contentAlignment = Alignment.Center) {
        // Under the button, so the button takes every press.
        if (waitsOnStorage) {
            AndroidView(
                modifier = Modifier.matchParentSize(),
                factory = { context -> StorageWaitArcView(context) },
                update = { arc -> arc.style(color = accent, label = waiting) },
            )
        }
        // Room around the glyph, so the arc turns clear of it at either size.
        IconButton(onClick = onToggle, modifier = Modifier.size(iconSize + arcClearance * 2)) {
            Icon(
                imageVector = if (isPlaying) Icons.Filled.Pause else Icons.Filled.PlayArrow,
                contentDescription = stringResource(if (isPlaying) R.string.pause else R.string.play),
                modifier = Modifier.size(iconSize),
            )
        }
    }
}
