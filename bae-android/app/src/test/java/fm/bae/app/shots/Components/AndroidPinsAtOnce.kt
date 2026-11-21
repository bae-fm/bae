package fm.bae.app.shots

import fm.bae.app.ui.downloads.AndroidPinsAtOnce
import org.junit.Test

/** The component shots of `AndroidPinsAtOnce`, one for each row of its preview. */
class AndroidPinsAtOnceShots : BoardShot() {
    @Test
    fun androidComponentPinsAtOnceThreeAtOnce() =
        component("android.component.pins-at-once.three-at-once", wide = true) { AndroidPinsAtOnce(value = 3u, limit = 6u, onSelect = {}) }

    @Test
    fun androidComponentPinsAtOnceOneOfFour() =
        component("android.component.pins-at-once.one-of-four", wide = true) { AndroidPinsAtOnce(value = 1u, limit = 4u, onSelect = {}) }
}
