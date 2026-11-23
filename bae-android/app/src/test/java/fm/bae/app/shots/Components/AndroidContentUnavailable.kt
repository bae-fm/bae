package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.AndroidContentUnavailable
import fm.bae.app.ui.library.ListPlaceholder
import org.junit.Test

/** The component shots of `AndroidContentUnavailable`, one for each row of its preview. */
class AndroidContentUnavailableShots : BoardShot() {
    @Test
    fun androidComponentContentUnavailableDefault() =
        component("android.component.content-unavailable.default") {
            // The preview's box, 412 by 140, in which the library pages' placeholder centres the line.
            InsidePreviewPage {
                Box(Modifier.size(412.dp, 140.dp)) { ListPlaceholder { AndroidContentUnavailable("No plays yet") } }
            }
        }
}
