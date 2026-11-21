package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidSection
import org.junit.Test

/** The component shots of `AndroidSection`, one for each row of its preview. */
class AndroidSectionShots : BoardShot() {
    @Test
    fun androidComponentSectionDefault() =
        component("android.component.section.default", wide = true) {
            Column {
                AndroidSection("People")
                Text("The card that follows", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }
}
