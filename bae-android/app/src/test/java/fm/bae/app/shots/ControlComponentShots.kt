package fm.bae.app.shots

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCheckbox
import fm.bae.app.ui.components.AndroidDialogActions
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidStep
import fm.bae.app.ui.components.AndroidStorageUsage
import fm.bae.app.ui.components.AndroidSwitch
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.StorageUsageSegment
import fm.bae.app.ui.components.TextFieldKind
import org.junit.Test
import uniffi.bae_bridge.BridgeStorageKind

/** The component shots of the controls: checkbox, switch, icon button, text field, dialog actions, step and storage usage. */
class ControlComponentShots : BoardShot() {
    @Test
    fun androidComponentCheckboxOn() =
        component("android.component.checkbox.on") { AndroidCheckbox("Pause between sides", checked = true, onCheckedChange = {}) }

    @Test
    fun androidComponentCheckboxOff() =
        component("android.component.checkbox.off") { AndroidCheckbox("Pause between sides", checked = false, onCheckedChange = {}) }

    @Test
    fun androidComponentSwitchOn() = component("android.component.switch.on") { RestoreOnLaunch(checked = true) }

    @Test
    fun androidComponentSwitchOff() = component("android.component.switch.off") { RestoreOnLaunch(checked = false) }

    @Test
    fun androidComponentIconButtonBack() =
        component("android.component.icon-button.back") { AndroidIconButton(Icons.AutoMirrored.Filled.ArrowBack, "Back", onClick = {}) }

    @Test
    fun androidComponentIconButtonSearch() =
        component("android.component.icon-button.search") { AndroidIconButton(Icons.Filled.Search, "Search", onClick = {}) }

    @Test
    fun androidComponentIconButtonMore() =
        component("android.component.icon-button.more") { AndroidIconButton(Icons.Filled.MoreVert, "More", onClick = {}) }

    @Test
    fun androidComponentTextFieldResting() =
        component("android.component.text-field.resting", wide = true) { NameField(value = "Home music") }

    @Test
    fun androidComponentTextFieldFocused() =
        component("android.component.text-field.focused", wide = true) {
            val focus = remember { FocusRequester() }
            NameField(value = "Home music", modifier = Modifier.focusRequester(focus))
            LaunchedEffect(Unit) { focus.requestFocus() }
        }

    @Test
    fun androidComponentTextFieldHelper() =
        component("android.component.text-field.helper", wide = true) {
            NameField(value = "Home music", helper = "People in this library see this name.")
        }

    @Test
    fun androidComponentTextFieldError() =
        component("android.component.text-field.error", wide = true) { NameField(value = "", helper = "Enter a name.", error = true) }

    @Test
    fun androidComponentTextFieldSecret() =
        component("android.component.text-field.secret", wide = true) { NameField(value = "Home music", kind = TextFieldKind.SECRET) }

    @Test
    fun androidComponentDialogActionsPlain() =
        component("android.component.dialog-actions.plain", wide = true) {
            AndroidDialogActions(action = DialogAction("Save", {}), dismiss = DialogAction("Cancel", {}))
        }

    @Test
    fun androidComponentDialogActionsDestructive() =
        component("android.component.dialog-actions.destructive", wide = true) {
            AndroidDialogActions(action = DialogAction("Save", {}, destructive = true), dismiss = DialogAction("Cancel", {}))
        }

    @Test
    fun androidComponentStepWithAField() =
        component("android.component.step.with-a-field", wide = true) {
            AndroidStep(title = "You are invited to Home music") {
                AndroidTextField(value = "Dan", onValueChange = {}, label = "Your name", helper = "Everyone in the library sees this name.")
                AndroidButton(onClick = {}, large = true) { Text("Join") }
            }
        }

    @Test
    fun androidComponentStepWithALine() =
        component("android.component.step.with-a-line", wide = true) {
            AndroidStep(title = "This invite has ended", text = "Ask Ben for a new one.") {
                AndroidButton(onClick = {}, large = true) { Text("OK") }
            }
        }

    @Test
    fun androidComponentStorageUsageUsual() =
        component("android.component.storage-usage.usual", wide = true) {
            Usage(
                "8.3 GB",
                segment(BridgeStorageKind.PINNED, "Pinned", "6.1 GB", 0.73f),
                segment(BridgeStorageKind.CACHE, "Cache", "1.8 GB", 0.22f),
                segment(BridgeStorageKind.LIBRARY, "Library", "420 MB", 0.05f),
            )
        }

    @Test
    fun androidComponentStorageUsageFromBeforeAReset() =
        component("android.component.storage-usage.from-before-a-reset", wide = true) {
            Usage(
                "9.3 GB",
                segment(BridgeStorageKind.PINNED, "Pinned", "6.1 GB", 0.66f),
                segment(BridgeStorageKind.CACHE, "Cache", "1.8 GB", 0.19f),
                segment(BridgeStorageKind.LIBRARY, "Library", "420 MB", 0.05f),
                segment(BridgeStorageKind.BEFORE_RESET, "From before the reset", "960 MB", 0.10f),
            )
        }

    @Test
    fun androidComponentStorageUsageNothingPinned() =
        component("android.component.storage-usage.nothing-pinned", wide = true) {
            Usage(
                "640 MB",
                segment(BridgeStorageKind.CACHE, "Cache", "220 MB", 0.34f),
                segment(BridgeStorageKind.LIBRARY, "Library", "420 MB", 0.66f),
            )
        }
}

/** The switch as the preview draws it, named for its setting. */
@Composable
private fun RestoreOnLaunch(checked: Boolean) =
    AndroidSwitch(
        checked = checked,
        onCheckedChange = {},
        modifier = Modifier.semantics { contentDescription = "Restore on launch" },
    )

@Composable
private fun NameField(
    value: String,
    modifier: Modifier = Modifier,
    helper: String? = null,
    error: Boolean = false,
    kind: TextFieldKind = TextFieldKind.PLAIN,
) = AndroidTextField(
    value = value,
    onValueChange = {},
    label = "Name",
    modifier = modifier,
    kind = kind,
    helper = helper,
    error = error,
)

@Composable
private fun Usage(
    total: String,
    vararg segments: StorageUsageSegment,
) = AndroidStorageUsage(total = total, of = "on this phone", segments = segments.toList())

private fun segment(
    kind: BridgeStorageKind,
    label: String,
    size: String,
    share: Float,
) = StorageUsageSegment(kind, label, size, share)
