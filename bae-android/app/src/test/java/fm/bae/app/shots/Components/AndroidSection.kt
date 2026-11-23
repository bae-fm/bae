package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidSection
import org.junit.Test

/** The component shots of `AndroidSection`, one for each row of its preview. */
class AndroidSectionShots : BoardShot() {
    @Test
    fun androidComponentSectionDefault() =
        component("android.component.section.default") {
            InsidePreviewPage {
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
                    InsideScreenEdges(top = ThemeSpace.related) {
                        AndroidSection("People", modifier = Modifier.padding(bottom = ThemeSpace.related))
                    }
                    PreviewSlot("The card that follows")
                }
            }
        }
}
