package fm.bae.app.shots

import org.junit.Test
import uniffi.bae_bridge.BridgeTrackSpan

/** The component shots of `AndroidSeekBar`, one for each row of its preview. */
class AndroidSeekBarShots : BoardShot() {
    @Test
    fun androidComponentSeekBarPlaying() = component("android.component.seek-bar.playing", wide = true) { playing.SeekRow() }

    @Test
    fun androidComponentSeekBarHoles() = component("android.component.seek-bar.holes", wide = true) { holes.SeekRow() }
}

private val holes =
    PlayerStaging(
        progress = 0.2,
        elapsedSeconds = 61,
        held = listOf(BridgeTrackSpan(0.0, 0.3), BridgeTrackSpan(0.55, 0.7)),
    )
