package fm.bae.app.shots

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidItem
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.StatusGlyph
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of `AndroidItem`, one for each row of its preview. */
class AndroidItemShots : BoardShot() {
    @Test
    fun androidComponentItemOneLine() =
        component("android.component.item.one-line") {
            AndroidItem(lead = { StatusGlyph(StatusTone.SUCCESS) }) { ItemText("Pinned on this phone") }
        }

    @Test
    fun androidComponentItemTwoLines() =
        component("android.component.item.two-lines") {
            AndroidItem(lead = { StatusGlyph(StatusTone.SUCCESS) }) {
                ItemText("Pinned on this phone. bae plays it from this phone without the network.")
            }
        }

    @Test
    fun androidComponentItemWithTrail() =
        component("android.component.item.with-trail") {
            AndroidItem(
                lead = { StatusGlyph(StatusTone.DANGER) },
                trail = { AndroidButton(kind = ButtonKind.TEXT, onClick = {}) { Text("Retry") } },
            ) { ItemText("Couldn’t pin") }
        }
}

@Composable
private fun ItemText(text: String) = Text(text, style = ThemeText.body.style)
