package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidDialogActions
import fm.bae.app.ui.components.DialogAction
import org.junit.Test

/** The component shots of `AndroidDialogActions`, one for each row of its preview. */
class AndroidDialogActionsShots : BoardShot() {
    @Test
    fun androidComponentDialogActionsPlain() =
        component("android.component.dialog-actions.plain") {
            PreviewActions(destructive = false)
        }

    @Test
    fun androidComponentDialogActionsDestructive() =
        component("android.component.dialog-actions.destructive") {
            PreviewActions(destructive = true)
        }
}

@Composable
private fun PreviewActions(destructive: Boolean) {
    Box(modifier = Modifier.fillMaxWidth().padding(top = ThemeSpace.related), contentAlignment = Alignment.TopEnd) {
        AndroidDialogActions(action = DialogAction("Save", {}, destructive = destructive), dismiss = DialogAction("Cancel", {}))
    }
}
