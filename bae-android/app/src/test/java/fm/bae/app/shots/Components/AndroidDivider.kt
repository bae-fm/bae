package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidDivider
import org.junit.Test

/** The component shots of `AndroidDivider`, one for each row of its preview. */
class AndroidDividerShots : BoardShot() {
    @Test
    fun androidComponentDividerDivider() =
        component("android.component.divider.divider") {
            Column {
                Text("Above", modifier = Modifier.fillMaxWidth().padding(vertical = ThemeSpace.related))
                AndroidDivider()
                Text("Below", modifier = Modifier.fillMaxWidth().padding(vertical = ThemeSpace.related))
            }
        }
}
