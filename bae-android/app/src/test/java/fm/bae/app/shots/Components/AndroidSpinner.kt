package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidSpinner
import org.junit.Test

/** The component shots of `AndroidSpinner`, one for each row of its preview. */
class AndroidSpinnerShots : BoardShot() {
    @Test
    fun androidComponentSpinnerAlone() = component("android.component.spinner.alone", wide = true) { AndroidSpinner() }

    @Test
    fun androidComponentSpinnerWithWords() =
        component("android.component.spinner.with-words", wide = true) { AndroidSpinner(text = "Looking for receivers…") }
}
