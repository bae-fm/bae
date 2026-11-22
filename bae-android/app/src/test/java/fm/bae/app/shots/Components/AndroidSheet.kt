package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.AndroidSheetHeading
import org.junit.Test

/** The component shots of `AndroidSheet`, one for each row of its preview. */
class AndroidSheetShots : BoardShot() {
    /** The preview pins the sheet 60 from the top of its cell and holds a padded slot in it. */
    @Test
    fun androidComponentSheetDefault() = component("android.component.sheet.default") { SheetOverScrim(top = 60.dp) { Slot() } }

    @Test
    fun androidComponentSheetNote() =
        component("android.component.sheet.note") {
            SheetOverScrim(
                AndroidSheetHeading("Not on this device", "On Ana’s Mac"),
                top = 60.dp,
            ) { Slot() }
        }

    @Test
    fun androidComponentSheetTitle() =
        component("android.component.sheet.title") {
            SheetOverScrim(AndroidSheetHeading("Not on this device"), top = 60.dp) {
                Box(modifier = Modifier.padding(16.dp).padding(12.dp)) {
                    Text("The sheet’s content", color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
}

@Composable
private fun Slot() =
    Box(modifier = Modifier.padding(16.dp).padding(12.dp)) {
        Text("The sheet’s content", color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
