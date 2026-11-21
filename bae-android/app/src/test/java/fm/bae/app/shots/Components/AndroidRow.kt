package fm.bae.app.shots

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.runtime.Composable
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.RowIcon
import fm.bae.app.ui.components.RowSwitch
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of `AndroidRow`, one for each row of its preview. */
class AndroidRowShots : BoardShot() {
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
                    icon = RowIcon.CHEVRON,
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
                    icon = RowIcon.EDIT,
                )
            }
        }

    @Test
    fun androidComponentRowValue() =
        component("android.component.row.value", wide = true) { RowInCard { AndroidRow(title = "Joined", value = "March 2026") } }

    @Test
    fun androidComponentRowControls() =
        component("android.component.row.controls", wide = true) {
            RowInCard { AndroidRow(title = "Sync", switch = RowSwitch(true) {}) }
        }

    @Test
    fun androidComponentRowTag() =
        component("android.component.row.tag", wide = true) {
            RowInCard {
                AndroidRow(title = "Sync", tag = "Member")
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
                AndroidRow(title = "Pause between sides and discs", switch = RowSwitch(true) {})
                AndroidRow(title = "Countdown", subtitle = "Off", nested = true)
            }
        }
}

/** A row in the card every row of a list is in. */
@Composable
private fun RowInCard(content: @Composable () -> Unit) = AndroidCard { content() }
