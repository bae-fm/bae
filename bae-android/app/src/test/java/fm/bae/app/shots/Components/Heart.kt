package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.Heart
import org.junit.Test

/** The component shot of `Heart`, its preview's Android row. */
class HeartShots : BoardShot() {
    @Test
    fun androidComponentHeartAndroidHearted() = component("android.component.heart.android-hearted") { Heart(on = true, size = 16.dp) }
}
