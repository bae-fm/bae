package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogHead
import org.junit.Test

/** The component shots of `AndroidDialog`, one for each row of its preview. */
class AndroidDialogShots : BoardShot() {
    /** The dialog's window isn't drawn in a test: its words and buttons are, on the surface a dialog has. */
    @Test
    fun androidComponentDialogDefault() =
        shot("android.component.dialog.default") {
            ShotTheme {
                DialogSurface {
                    Column(modifier = Modifier.width(DIALOG_WIDTH), verticalArrangement = Arrangement.spacedBy(ThemeSpace.section)) {
                        DialogHead(
                            title = "Remove library from this device?",
                            message = "The library will remain in the cloud and on your other devices.",
                        )
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related, Alignment.End),
                        ) {
                            AndroidButton(onClick = {}, kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL) { Text("Cancel") }
                            AndroidButton(onClick = {}, kind = ButtonKind.TEXT, tone = ButtonTone.DESTRUCTIVE) { Text("Remove") }
                        }
                    }
                }
            }
        }
}

private val DIALOG_WIDTH = 340.dp
