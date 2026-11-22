package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.Heart
import org.junit.Test

/** The component shots of `Heart`, one for each row of its preview. */
class HeartShots : BoardShot() {
    @Test
    fun androidComponentHeartHearted() = component("android.component.heart.hearted") { Heart(on = true, size = 16.dp) }

    @Test
    fun androidComponentHeartNotHearted() = component("android.component.heart.not-hearted") { Heart(on = false, size = 16.dp) }
}
