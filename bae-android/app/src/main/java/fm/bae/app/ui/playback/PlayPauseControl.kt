package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme

/** Size parameters for [PlayPauseControl]. Compact bar and expanded player use different sizes. */
data class PlayPauseControlSizes(
    val iconSize: Dp,
    val spinnerSize: Dp,
    val spinnerStroke: Dp,
)

/**
 * Play/pause toggle, replaced by a spinner while core is preparing or buffering
 * the track (initial load, or a seek to a position not yet downloaded). Shared
 * by the compact [NowPlayingBar] and the full-screen [ExpandedNowPlayingScreen],
 * which differ only in icon/spinner size. The spinner sits in a 48dp box (the
 * IconButton footprint) so swapping it in doesn't reflow the transport row.
 */
@Composable
fun PlayPauseControl(
    isPlaying: Boolean,
    isLoading: Boolean,
    sizes: PlayPauseControlSizes,
    onToggle: () -> Unit,
) {
    if (isLoading) {
        Box(modifier = Modifier.size(48.dp), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(
                modifier = Modifier.size(sizes.spinnerSize),
                strokeWidth = sizes.spinnerStroke,
            )
        }
    } else {
        IconButton(onClick = onToggle) {
            Icon(
                imageVector = if (isPlaying) Icons.Filled.Pause else Icons.Filled.PlayArrow,
                contentDescription = stringResource(if (isPlaying) R.string.pause else R.string.play),
                modifier = Modifier.size(sizes.iconSize),
            )
        }
    }
}

private val previewSizes = PlayPauseControlSizes(iconSize = 48.dp, spinnerSize = 36.dp, spinnerStroke = 3.dp)

@Preview(showBackground = true)
@Composable
private fun PlayPauseControlPlayingPreview() {
    BaeTheme {
        PlayPauseControl(isPlaying = true, isLoading = false, sizes = previewSizes, onToggle = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun PlayPauseControlLoadingPreview() {
    BaeTheme {
        PlayPauseControl(isPlaying = false, isLoading = true, sizes = previewSizes, onToggle = {})
    }
}
