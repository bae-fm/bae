package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.Stars
import org.junit.Test
import uniffi.bae_bridge.BridgeStars

/** The component shots of `Stars`, one for each row of its preview. */
class StarsShots : BoardShot() {
    @Test
    fun androidComponentStarsRated() = component("android.component.stars.rated") { Stars(value = 4, size = 16.dp) }

    @Test
    fun androidComponentStarsNotRated() = component("android.component.stars.not-rated") { Stars(value = 0, size = 16.dp) }

    @Test
    fun androidComponentStarsNumberAndOneStar() =
        component("android.component.stars.number-and-one-star") { Stars(rating = BridgeStars.FOUR, size = 16.dp) }
}
