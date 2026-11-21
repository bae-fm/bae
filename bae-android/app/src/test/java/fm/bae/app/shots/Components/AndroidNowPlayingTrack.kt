package fm.bae.app.shots

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.playback.AndroidNowPlayingTrack
import org.junit.Test

/** The component shots of `AndroidNowPlayingTrack`, one for each row of its preview. */
class AndroidNowPlayingTrackShots : BoardShot() {
    @Test
    fun androidComponentNowPlayingTrackPlaying() = component("android.component.now-playing-track.playing") { Track(playing) }

    @Test
    fun androidComponentNowPlayingTrackNoCover() = component("android.component.now-playing-track.no-cover") { Track(withoutCover) }

    @Test
    fun androidComponentNowPlayingTrackLongTitle() =
        component("android.component.now-playing-track.long-title") {
            Row(modifier = Modifier.width(260.dp)) { Track(longTitled, Modifier.weight(1f)) }
        }
}

@Composable
private fun Track(
    staging: PlayerStaging,
    modifier: Modifier = Modifier,
) = Images { AndroidNowPlayingTrack(staging.track, onExpand = {}, modifier = modifier) }
