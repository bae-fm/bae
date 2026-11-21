package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.playback.AndroidPlayPause
import org.junit.Test

/** The component shots of `AndroidPlayPause`, one for each row of its preview. */
class AndroidPlayPauseShots : BoardShot() {
    @Test
    fun androidComponentPlayPausePaused() =
        component("android.component.play-pause.paused") { AndroidPlayPause(false, ThemeIcon.large, false) {} }

    @Test
    fun androidComponentPlayPausePlaying() =
        component("android.component.play-pause.playing") { AndroidPlayPause(true, ThemeIcon.large, false) {} }

    @Test
    fun androidComponentPlayPauseWaiting() =
        component("android.component.play-pause.waiting") { AndroidPlayPause(true, ThemeIcon.large, waiting.waitsOnStorage) {} }

    @Test
    fun androidComponentPlayPauseFullPlayer() =
        component("android.component.play-pause.full-player") { AndroidPlayPause(true, 48.dp, false) {} }
}
