package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidCheckbox
import org.junit.Test

/** The component shots of `AndroidCheckbox`, one for each row of its preview. */
class AndroidCheckboxShots : BoardShot() {
    @Test
    fun androidComponentCheckboxOn() =
        component("android.component.checkbox.on") { AndroidCheckbox("Pause between sides", checked = true, onCheckedChange = {}) }

    @Test
    fun androidComponentCheckboxOff() =
        component("android.component.checkbox.off") { AndroidCheckbox("Pause between sides", checked = false, onCheckedChange = {}) }
}
