package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidErrorLine
import org.junit.Test

/** The component shots of `AndroidErrorLine`, one for each row of its preview. */
class AndroidErrorLineShots : BoardShot() {
    @Test
    fun androidComponentErrorLineError() =
        component("android.component.error-line.error") { AndroidErrorLine("bae couldn’t read the queue.") }

    @Test
    fun androidComponentErrorLineLongError() =
        component("android.component.error-line.long-error") {
            AndroidErrorLine("bae couldn’t read the queue. The library file on this phone is damaged; open the library again to repair it.")
        }
}
