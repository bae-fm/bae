package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidEyebrow
import org.junit.Test

/** The component shots of `AndroidEyebrow`, one for each row of its preview. */
class AndroidEyebrowShots : BoardShot() {
    @Test
    fun androidComponentEyebrowDefault() = component("android.component.eyebrow.default") { AndroidEyebrow("Side A") }
}
