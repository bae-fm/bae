package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidMatchCode
import org.junit.Test

/** The component shots of `AndroidMatchCode`, one for each row of its preview. */
class AndroidMatchCodeShots : BoardShot() {
    @Test
    fun androidComponentMatchCodeOnAScreen() =
        component("android.component.match-code.on-a-screen") {
            AndroidMatchCode("412 778", waiting = "Waiting for your other device…")
        }

    @Test
    fun androidComponentMatchCodeInADialog() =
        component("android.component.match-code.in-a-dialog") { AndroidMatchCode("412 778", dialog = true) }
}
