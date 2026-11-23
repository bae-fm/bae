package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.sp
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
                    Text(
                        "The card that follows",
                        modifier = Modifier.padding(ThemeSpace.group + ThemeSpace.hairline),
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
}
