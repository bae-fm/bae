package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.vectorResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.ButtonAlign
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import org.junit.Test

/** The component shots of `AndroidButton`, one for each row of its preview. */
class AndroidButtonShots : BoardShot() {
    @Test
    fun androidComponentButtonFilled() = component("android.component.button.filled") { JoinButton() }

    @Test
    fun androidComponentButtonOutlined() = component("android.component.button.outlined") { JoinButton(ButtonKind.OUTLINED) }

    @Test
    fun androidComponentButtonText() = component("android.component.button.text") { JoinButton(ButtonKind.TEXT) }

    @Test
    fun androidComponentButtonDestructive() =
        component("android.component.button.destructive") { JoinButton(tone = ButtonTone.DESTRUCTIVE) }

    @Test
    fun androidComponentButtonOff() = component("android.component.button.off") { JoinButton(enabled = false) }

    @Test
    fun androidComponentButtonLarge() = component("android.component.button.large") { JoinButton(large = true) }

    @Test
    fun androidComponentButtonUnderText() =
        component("android.component.button.under-text") {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                Text(
                    "They will use your key.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                AndroidButton(onClick = {}, kind = ButtonKind.TEXT, alignTo = ButtonAlign.TEXT) { Text("Use their own key instead") }
            }
        }

    @Test
    fun androidComponentButtonUnderAField() =
        component("android.component.button.under-a-field") {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                AndroidTextField(value = "AKIAEXAMPLEKEY", onValueChange = {}, label = "Access key ID")
                AndroidButton(onClick = {}, kind = ButtonKind.TEXT, alignTo = ButtonAlign.FIELD) { Text("Use my key instead") }
            }
        }

    @Test
    fun androidComponentButtonWithIcon() =
        component("android.component.button.with-icon") {
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = {}, icon = ImageVector.vectorResource(R.drawable.ic_keep)) { Text("Pin") }
        }

    @Test
    fun androidComponentButtonTonal() = component("android.component.button.tonal") { JoinButton(ButtonKind.TONAL) }

    @Test
    fun androidComponentButtonNeutral() =
        component("android.component.button.neutral") { JoinButton(ButtonKind.TEXT, tone = ButtonTone.NEUTRAL) }
}

@Composable
private fun JoinButton(
    kind: ButtonKind = ButtonKind.FILLED,
    tone: ButtonTone = ButtonTone.DEFAULT,
    large: Boolean = false,
    enabled: Boolean = true,
) = AndroidButton(onClick = {}, kind = kind, tone = tone, large = large, enabled = enabled) { Text("Join library") }
