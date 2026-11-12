package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidEyebrow
import fm.bae.app.ui.components.AndroidMatchCode
import fm.bae.app.ui.components.AndroidRadio
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonAlign
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogHead
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of the design system's basic parts: buttons, cards and rows, radios, headings, spinners, dialogs and sheets. */
class BasicComponentShots : BoardShot() {
    @Test
    fun androidComponentButtonFilled() = component("android.component.button.filled") { JoinButton() }

    @Test
    fun androidComponentButtonOutlined() = component("android.component.button.outlined") { JoinButton(ButtonKind.OUTLINED) }

    @Test
    fun androidComponentButtonText() = component("android.component.button.text") { JoinButton(ButtonKind.TEXT) }

    @Test
    fun androidComponentButtonDestructive() =
        component("android.component.button.destructive") { JoinButton(tone = ButtonTone.DESTRUCTIVE) }

    @Test
    fun androidComponentButtonOff() = component("android.component.button.off") { JoinButton(enabled = false) }

    @Test
    fun androidComponentButtonLarge() = component("android.component.button.large", wide = true) { JoinButton(large = true) }

    @Test
    fun androidComponentButtonUnderText() =
        component("android.component.button.under-text", wide = true) {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                Text(
                    "They’ll use your key.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                AndroidButton(onClick = {}, kind = ButtonKind.TEXT, alignTo = ButtonAlign.TEXT) { Text("Use their own key instead") }
            }
        }

    @Test
    fun androidComponentButtonUnderAField() =
        component("android.component.button.under-a-field", wide = true) {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                OutlinedTextField(value = "AKIAEXAMPLEKEY", onValueChange = {}, label = { Text("Access key ID") })
                AndroidButton(onClick = {}, kind = ButtonKind.TEXT, alignTo = ButtonAlign.FIELD) { Text("Use my key instead") }
            }
        }

    @Test
    fun androidComponentCardRows() =
        component("android.component.card.rows", wide = true) {
            AndroidCard {
                AndroidRow(title = "Phone")
                AndroidRow(title = "Laptop")
            }
        }

    @Test
    fun androidComponentRowTitle() = component("android.component.row.title", wide = true) { RowInCard { AndroidRow(title = "Sync") } }

    @Test
    fun androidComponentRowSubtitle() =
        component("android.component.row.subtitle", wide = true) { RowInCard { AndroidRow(title = "Sync", subtitle = "Synced") } }

    @Test
    fun androidComponentRowTone() =
        component("android.component.row.tone", wide = true) {
            RowInCard { AndroidRow(title = "Sync", subtitle = "Synced", tone = StatusTone.SUCCESS) }
        }

    @Test
    fun androidComponentRowOpensAPage() =
        component("android.component.row.opens-a-page", wide = true) {
            RowInCard {
                AndroidRow(
                    title = "Sync",
                    onClick = {},
                    trailing = {
                        Icon(
                            Icons.AutoMirrored.Filled.KeyboardArrowRight,
                            null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    },
                )
            }
        }

    @Test
    fun androidComponentRowOpensADialog() =
        component("android.component.row.opens-a-dialog", wide = true) {
            RowInCard {
                AndroidRow(
                    title = "Sync",
                    onClick = {},
                    trailing = { Icon(Icons.Filled.Edit, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
                )
            }
        }

    @Test
    fun androidComponentRowValue() =
        component("android.component.row.value", wide = true) { RowInCard { AndroidRow(title = "Joined", value = "March 2026") } }

    @Test
    fun androidComponentRowControls() =
        component("android.component.row.controls", wide = true) {
            RowInCard { AndroidRow(title = "Sync", trailing = { Switch(checked = true, onCheckedChange = {}) }) }
        }

    @Test
    fun androidComponentRowTag() =
        component("android.component.row.tag", wide = true) {
            RowInCard {
                AndroidRow(title = "Sync", trailing = { Text("Member", color = MaterialTheme.colorScheme.onSurfaceVariant) })
            }
        }

    @Test
    fun androidComponentRowAction() =
        component("android.component.row.action", wide = true) { RowInCard { AndroidRow(title = "Sync", action = true, onClick = {}) } }

    @Test
    fun androidComponentRowActionThatAdds() =
        component("android.component.row.action-that-adds", wide = true) {
            RowInCard { AndroidRow(title = "Sync", action = true, leadingIcon = Icons.Filled.Add, onClick = {}) }
        }

    @Test
    fun androidComponentRowDestructiveAction() =
        component("android.component.row.destructive-action", wide = true) {
            RowInCard { AndroidRow(title = "Sync", action = true, destructive = true, onClick = {}) }
        }

    @Test
    fun androidComponentRowSelected() =
        component("android.component.row.selected", wide = true) { RowInCard { AndroidRow(title = "Sync", selected = true) } }

    @Test
    fun androidComponentRowNested() =
        component("android.component.row.nested", wide = true) {
            RowInCard {
                AndroidRow(title = "Pause between sides and discs", trailing = { Switch(checked = true, onCheckedChange = {}) })
                AndroidRow(title = "Countdown", subtitle = "Off", nested = true)
            }
        }

    @Test
    fun androidComponentRadioChecked() =
        component("android.component.radio.checked") { AndroidRadio("Only me", checked = true, onClick = {}) }

    @Test
    fun androidComponentRadioUnchecked() =
        component("android.component.radio.unchecked") { AndroidRadio("Only me", checked = false, onClick = {}) }

    @Test
    fun androidComponentSectionDefault() = component("android.component.section.default", wide = true) { AndroidSection("People") }

    @Test
    fun androidComponentEyebrowDefault() = component("android.component.eyebrow.default") { AndroidEyebrow("Side A") }

    @Test
    fun androidComponentSpinnerAlone() = component("android.component.spinner.alone", wide = true) { AndroidSpinner() }

    @Test
    fun androidComponentSpinnerWithWords() =
        component("android.component.spinner.with-words", wide = true) { AndroidSpinner(text = "Looking for receivers…") }

    @Test
    fun androidComponentMatchCodeInADialog() =
        component("android.component.match-code.in-a-dialog", wide = true) { AndroidMatchCode("412 778") }

    /** The dialog's window isn't drawn in a test: its words and buttons are, on the surface a dialog has. */
    @Test
    fun androidComponentDialogDefault() =
        shot("android.component.dialog.default") {
            BaeTheme {
                DialogSurface {
                    Column(modifier = Modifier.width(DIALOG_WIDTH), verticalArrangement = Arrangement.spacedBy(ThemeSpace.section)) {
                        DialogHead(
                            title = "Remove library from this device?",
                            message = "The library will remain in the cloud and on your other devices.",
                        )
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related, Alignment.End),
                        ) {
                            AndroidButton(onClick = {}, kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL) { Text("Cancel") }
                            AndroidButton(onClick = {}, kind = ButtonKind.TEXT, tone = ButtonTone.DESTRUCTIVE) { Text("Remove") }
                        }
                    }
                }
            }
        }

    @Test
    fun androidComponentSheetDefault() = shot("android.component.sheet.default") { SheetScene(null) { Text("The sheet’s content") } }
}

private val DIALOG_WIDTH = 340.dp

@Composable
private fun JoinButton(
    kind: ButtonKind = ButtonKind.FILLED,
    tone: ButtonTone = ButtonTone.DEFAULT,
    large: Boolean = false,
    enabled: Boolean = true,
) = AndroidButton(onClick = {}, kind = kind, tone = tone, large = large, enabled = enabled) { Text("Join library") }

/** A row in the card every row of a list is in. */
@Composable
private fun RowInCard(content: @Composable () -> Unit) = AndroidCard { content() }
