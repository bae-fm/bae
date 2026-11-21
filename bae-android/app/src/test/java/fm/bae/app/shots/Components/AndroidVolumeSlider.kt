package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.playback.AndroidVolumeSlider
import org.junit.Test

/** The component shots of `AndroidVolumeSlider`, one for each row of its preview. */
class AndroidVolumeSliderShots : BoardShot() {
    @Test
    fun androidComponentVolumeSliderMostOfTheWay() = component("android.component.volume-slider.most-of-the-way") { Volume(0.7f) }

    @Test
    fun androidComponentVolumeSliderLow() = component("android.component.volume-slider.low") { Volume(0.25f) }

    @Test
    fun androidComponentVolumeSliderSilent() = component("android.component.volume-slider.silent") { Volume(0f) }

    @Test
    fun androidComponentVolumeSliderFull() = component("android.component.volume-slider.full") { Volume(1f) }
}

@Composable
private fun Volume(level: Float) = AndroidVolumeSlider(level, {}, {})
