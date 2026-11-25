package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.Heart
import org.junit.Test

/** The component shots of `Heart`, one for each Android row of its preview. */
class HeartShots : BoardShot() {
    @Test
    fun androidComponentHeartAndroidHearted() = component("android.component.heart.android-hearted") { Heart(on = true, size = 16.dp) }

    @Test
    fun androidComponentHeartAndroidNotHearted() =
        component("android.component.heart.android-not-hearted") { Heart(on = false, size = 16.dp) }
}
