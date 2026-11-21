package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.playback.AndroidPlaceholderLine
import org.junit.Test

/** The component shots of `AndroidPlaceholderLine`, one for each row of its preview. */
class AndroidPlaceholderLineShots : BoardShot() {
    @Test
    fun androidComponentPlaceholderLineLine() =
        component("android.component.placeholder-line.line") { AndroidPlaceholderLine(width = 160.dp, height = 12.dp) }

    @Test
    fun androidComponentPlaceholderLineShortLine() =
        component("android.component.placeholder-line.short-line") { AndroidPlaceholderLine(width = 100.dp, height = 10.dp) }

    @Test
    fun androidComponentPlaceholderLinePicture() =
        component("android.component.placeholder-line.picture") { AndroidPlaceholderLine(width = 48.dp, height = 48.dp) }
}
