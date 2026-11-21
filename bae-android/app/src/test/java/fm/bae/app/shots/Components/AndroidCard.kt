package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import org.junit.Test

/** The component shots of `AndroidCard`, one for each row of its preview. */
class AndroidCardShots : BoardShot() {
    @Test
    fun androidComponentCardRows() =
        component("android.component.card.rows") {
            // The board's card carries its own margin: 16 at the sides and 8 above.
            Box(modifier = Modifier.padding(start = ThemeSpace.edge, top = ThemeSpace.related, end = ThemeSpace.edge)) {
                AndroidCard {
                    AndroidRow(title = "Phone")
                    AndroidRow(title = "Laptop")
                }
            }
        }
}
