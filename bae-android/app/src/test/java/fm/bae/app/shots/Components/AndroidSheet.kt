package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import org.junit.Test

/** The component shots of `AndroidSheet`, one for each row of its preview. */
class AndroidSheetShots : BoardShot() {
    /** The preview pins the sheet 60 from the top of its cell and holds a padded slot in it. */
    @Test
    fun androidComponentSheetDefault() =
        component("android.component.sheet.default") {
            SheetOverScrim(null, top = 60.dp) {
                Box(modifier = Modifier.padding(16.dp).padding(12.dp)) {
                    Text("The sheet’s content", color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
}
