package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.AndroidPinField
import fm.bae.app.ui.components.focusOnShow
import org.junit.Test

/** The component shots of `AndroidPinField`, one for each row of its preview. */
class AndroidPinFieldShots : BoardShot() {
    @Test
    fun androidComponentPinFieldEmpty() = component("android.component.pin-field.empty") { InPreviewCell(4) { AndroidPinField(4, "", {}) } }

    @Test
    fun androidComponentPinFieldFocused() =
        component("android.component.pin-field.focused") { InPreviewCell(4) { AndroidPinField(4, "48", {}, Modifier.focusOnShow()) } }

    @Test
    fun androidComponentPinFieldFilled() =
        component("android.component.pin-field.filled") {
            InPreviewCell(4) { AndroidPinField(4, "4821", {}) }
        }

    @Test
    fun androidComponentPinFieldError() =
        component(
            "android.component.pin-field.error",
        ) { InPreviewCell(4) { AndroidPinField(4, "4821", {}, error = "That PIN did not match.") } }

    @Test
    fun androidComponentPinFieldOff() =
        component("android.component.pin-field.off") {
            InPreviewCell(4) { AndroidPinField(4, "4821", {}, enabled = false) }
        }

    @Test
    fun androidComponentPinFieldSixDigits() =
        component("android.component.pin-field.six-digits") {
            InPreviewCell(6) { AndroidPinField(6, "482", {}) }
        }
}

/** The preview's cell, as wide as the field's [digits] boxes and the gaps between them. */
@Composable
private fun InPreviewCell(
    digits: Int,
    content: @Composable () -> Unit,
) = Box(Modifier.width((48 * digits + 12 * (digits - 1)).dp)) { content() }
