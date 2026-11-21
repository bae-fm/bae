package fm.bae.app.shots

import androidx.compose.ui.Modifier
import fm.bae.app.ui.components.AndroidPinField
import fm.bae.app.ui.components.focusOnShow
import org.junit.Test

/** The component shots of `AndroidPinField`, one for each row of its preview. */
class AndroidPinFieldShots : BoardShot() {
    @Test
    fun androidComponentPinFieldEmpty() = component("android.component.pin-field.empty") { AndroidPinField(4, "", {}) }

    @Test
    fun androidComponentPinFieldFocused() =
        component("android.component.pin-field.focused") { AndroidPinField(4, "48", {}, Modifier.focusOnShow()) }

    @Test
    fun androidComponentPinFieldFilled() = component("android.component.pin-field.filled") { AndroidPinField(4, "4821", {}) }

    @Test
    fun androidComponentPinFieldError() =
        component("android.component.pin-field.error") { AndroidPinField(4, "4821", {}, error = "That PIN did not match.") }

    @Test
    fun androidComponentPinFieldOff() = component("android.component.pin-field.off") { AndroidPinField(4, "4821", {}, enabled = false) }

    @Test
    fun androidComponentPinFieldSixDigits() = component("android.component.pin-field.six-digits") { AndroidPinField(6, "482", {}) }
}
