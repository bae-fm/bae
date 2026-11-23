package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.settings.AndroidAccentPicker
import org.junit.Test

/** The component shots of `AndroidAccentPicker`, one for each row of its preview. */
class AndroidAccentPickerShots : BoardShot() {
    @Test
    fun androidComponentAccentPickerResting() =
        component("android.component.accent-picker.resting") {
            // The board's card carries its own margin: 16 at the sides and 8 above.
            Box(Modifier.padding(start = ThemeSpace.edge, top = ThemeSpace.related, end = ThemeSpace.edge)) {
                AndroidCard {
                    Box(Modifier.fillMaxWidth().padding(ThemeSpace.edge)) {
                        AndroidAccentPicker(AccentChoice.PINK) {}
                    }
                }
            }
        }
}
