package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidRow
import org.junit.Test

/** The component shots of `AndroidNote`, one for each row of its preview. */
class AndroidNoteShots : BoardShot() {
    @Test
    fun androidComponentNoteDefault() =
        component("android.component.note.default") {
            // The board's card carries its own margin: 16 at the sides and 8 above.
            Column(
                modifier = Modifier.padding(start = ThemeSpace.edge, top = ThemeSpace.related, end = ThemeSpace.edge),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                AndroidCard { AndroidRow(title = "Restore code", subtitle = "Shown") }
                AndroidNote("Anyone with this code can open the library.")
            }
        }
}
