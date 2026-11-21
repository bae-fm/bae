package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.Stars
import org.junit.Test

/** The component shots of `Stars`, one for each row of its preview. */
class StarsShots : BoardShot() {
    @Test
    fun androidComponentStarsRated() = component("android.component.stars.rated") { Stars(value = 4, size = 16.dp) }

    @Test
    fun androidComponentStarsNotRated() = component("android.component.stars.not-rated") { Stars(value = 0, size = 16.dp) }
}
