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
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.R
import fm.bae.app.ui.PreviewTheme
import fm.bae.fastvalues.LatestSource

/** How far the arc sits outside the glyph, on each side. */
private val arcClearance = 8.dp

/**
 * Play/pause toggle, its glyph [iconSize], which differs between the bar and
 * the expanded player. While core loads a track it shows pause, and a press
 * lands the load paused; while the load waits on storage, as [storageWait]
 * holds, an arc turns around it.
 */
@Composable
fun AndroidPlayPause(
    isPlaying: Boolean,
    iconSize: Dp,
    storageWait: LatestSource<Boolean>?,
    onToggle: () -> Unit,
) {
    val accent = MaterialTheme.colorScheme.primary.toArgb()
    val waiting = stringResource(R.string.downloading_the_track)
    Box(contentAlignment = Alignment.Center) {
        // Under the button, so the button takes every press.
        if (storageWait != null) {
            AndroidView(
                modifier = Modifier.matchParentSize(),
                factory = { context -> StorageWaitArcView(context) },
                update = { arc ->
                    arc.style(color = accent, label = waiting)
                    arc.follow(storageWait)
                },
                onRelease = { arc -> arc.stopFollowing() },
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

@Preview(showBackground = true)
@Composable
private fun PlayPauseControlPlayingPreview() {
    PreviewTheme {
        AndroidPlayPause(isPlaying = true, iconSize = 48.dp, storageWait = null, onToggle = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun PlayPauseControlPausedPreview() {
    PreviewTheme {
        AndroidPlayPause(isPlaying = false, iconSize = 48.dp, storageWait = null, onToggle = {})
    }
}
