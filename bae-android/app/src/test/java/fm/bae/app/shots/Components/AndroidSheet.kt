package fm.bae.app.shots

import androidx.compose.material3.Text
import org.junit.Test

/** The component shots of `AndroidSheet`, one for each row of its preview. */
class AndroidSheetShots : BoardShot() {
    @Test
    fun androidComponentSheetDefault() =
        component("android.component.sheet.default") { SheetOverScrim(null) { Text("The sheet’s content") } }
}
