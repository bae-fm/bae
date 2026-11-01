package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme

/**
 * Play/pause toggle, its glyph [iconSize], which differs between the bar and
 * the expanded player. While core loads a track it shows pause, and a press
 * lands the load paused.
 */
@Composable
fun PlayPauseControl(
    isPlaying: Boolean,
    iconSize: Dp,
    onToggle: () -> Unit,
) {
    IconButton(onClick = onToggle) {
        Icon(
            imageVector = if (isPlaying) Icons.Filled.Pause else Icons.Filled.PlayArrow,
            contentDescription = stringResource(if (isPlaying) R.string.pause else R.string.play),
            modifier = Modifier.size(iconSize),
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun PlayPauseControlPlayingPreview() {
    BaeTheme {
        PlayPauseControl(isPlaying = true, iconSize = 48.dp, onToggle = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun PlayPauseControlPausedPreview() {
    BaeTheme {
        PlayPauseControl(isPlaying = false, iconSize = 48.dp, onToggle = {})
    }
}
