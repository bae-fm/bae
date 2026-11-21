package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidDialogActions
import fm.bae.app.ui.components.DialogAction
import org.junit.Test

/** The component shots of `AndroidDialogActions`, one for each row of its preview. */
class AndroidDialogActionsShots : BoardShot() {
    @Test
    fun androidComponentDialogActionsPlain() =
        component("android.component.dialog-actions.plain") {
            AndroidDialogActions(action = DialogAction("Save", {}), dismiss = DialogAction("Cancel", {}))
        }

    @Test
    fun androidComponentDialogActionsDestructive() =
        component("android.component.dialog-actions.destructive") {
            AndroidDialogActions(action = DialogAction("Save", {}, destructive = true), dismiss = DialogAction("Cancel", {}))
        }
}
