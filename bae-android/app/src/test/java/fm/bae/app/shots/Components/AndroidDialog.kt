package fm.bae.app.shots

import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.WIDE_DIALOG_WIDTH
import org.junit.Test

/** The component shots of `AndroidDialog`, one for each row of its preview. */
class AndroidDialogShots : BoardShot() {
    /** The dialog's window isn't drawn in a test: its words and buttons are, on the surface a dialog has. */
    @Test
    fun androidComponentDialogDefault() =
        component("android.component.dialog.default") {
            DialogInCell(
                DialogParts(
                    title = "Remove library from this device?",
                    message = "The library will remain in the cloud and on your other devices.",
                    dismiss = DialogAction("Cancel", {}),
                    action = DialogAction("Remove", {}),
                    width = WIDE_DIALOG_WIDTH,
                ),
            )
        }
}
