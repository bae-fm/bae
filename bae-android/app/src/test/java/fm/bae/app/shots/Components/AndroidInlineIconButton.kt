package fm.bae.app.shots

import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidInlineIconButton
import org.junit.Test

/** The component shots of `AndroidInlineIconButton`, one for each row of its preview. */
class AndroidInlineIconButtonShots : BoardShot() {
    @Test
    fun androidComponentInlineIconButtonBesideALine() =
        component("android.component.inline-icon-button.beside-a-line") {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Low Tide Hours", style = ThemeText.rowTitle.style, modifier = Modifier.weight(1f))
                AndroidInlineIconButton(SymbolIcons.Close, "Cancel", onClick = {})
            }
        }
}
