package fm.bae.app.shots

import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.AndroidSpinner
import org.junit.Test

/** The component shots of `AndroidSpinner`, one for each row of its preview. */
class AndroidSpinnerShots : BaeBoardShot() {
    @Test
    fun androidComponentSpinnerAlone() = component("android.component.spinner.alone", enterMillis = SPINNER_ARC_MILLIS) { AndroidSpinner() }

    @Test
    fun androidComponentSpinnerWithWords() =
        component("android.component.spinner.with-words", enterMillis = SPINNER_ARC_MILLIS) {
            AndroidSpinner(text = "Looking for receivers…")
        }

    @Test
    fun androidComponentSpinnerOnTheBackdrop() =
        component("android.component.spinner.on-the-backdrop", enterMillis = SPINNER_ARC_MILLIS) {
            OnTheBackdrop { AndroidSpinner(color = BaeTheme.colors.onFill) }
        }
}
