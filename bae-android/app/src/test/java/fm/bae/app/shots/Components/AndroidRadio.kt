package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidRadio
import org.junit.Test

/** The component shots of `AndroidRadio`, one for each row of its preview. */
class AndroidRadioShots : BoardShot() {
    @Test
    fun androidComponentRadioChecked() =
        component("android.component.radio.checked") { HuggingContent { AndroidRadio("Only me", checked = true, onClick = {}) } }

    @Test
    fun androidComponentRadioUnchecked() =
        component("android.component.radio.unchecked") { HuggingContent { AndroidRadio("Only me", checked = false, onClick = {}) } }
}
