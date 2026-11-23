package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidDisclosure
import fm.bae.app.ui.components.AndroidRow
import org.junit.Test

/** The component shots of `AndroidDisclosure`, one for each row of its preview. */
class AndroidDisclosureShots : BoardShot() {
    @Test
    fun androidComponentDisclosureClosed() =
        component("android.component.disclosure.closed") {
            HuggingContent { AndroidDisclosure(label = "Removed devices (2)", open = false, onToggle = {}) {} }
        }

    @Test
    fun androidComponentDisclosureOpen() =
        component("android.component.disclosure.open") {
            HuggingContent {
                AndroidDisclosure(label = "Removed devices (2)", open = true, onToggle = {}) {
                    AndroidRow(title = "Odile’s phone", subtitle = "Removed")
                    AndroidRow(title = "Kitchen tablet", subtitle = "Replaced")
                }
            }
        }
}
