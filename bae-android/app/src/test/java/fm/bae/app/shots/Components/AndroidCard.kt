package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import org.junit.Test

/** The component shots of `AndroidCard`, one for each row of its preview. */
class AndroidCardShots : BoardShot() {
    @Test
    fun androidComponentCardRows() =
        component("android.component.card.rows") {
            AndroidCard {
                AndroidRow(title = "Phone")
                AndroidRow(title = "Laptop")
            }
        }
}
