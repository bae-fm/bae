package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.RowIcon
import fm.bae.app.ui.components.RowSwitch
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of `AndroidRow`, one for each row of its preview. */
class AndroidRowShots : BoardShot() {
    @Test
    fun androidComponentRowTitle() = component("android.component.row.title") { RowInCard { AndroidRow(title = "Sync") } }

    @Test
    fun androidComponentRowSubtitle() =
        component("android.component.row.subtitle") { RowInCard { AndroidRow(title = "Sync", subtitle = "Synced") } }

    @Test
    fun androidComponentRowTone() =
        component("android.component.row.tone") {
            RowInCard { AndroidRow(title = "Sync", subtitle = "Synced", tone = StatusTone.SUCCESS) }
        }

    @Test
    fun androidComponentRowOpensAPage() =
        component("android.component.row.opens-a-page") {
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
        component("android.component.row.opens-a-dialog") {
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
        component("android.component.row.value") { RowInCard { AndroidRow(title = "Joined", value = "March 2026") } }

    @Test
    fun androidComponentRowControls() =
        component("android.component.row.controls") {
            RowInCard { AndroidRow(title = "Sync", switch = RowSwitch(true) {}) }
        }

    @Test
    fun androidComponentRowTag() =
        component("android.component.row.tag") {
            RowInCard {
                AndroidRow(title = "Sync", tag = "Member")
            }
        }

    @Test
    fun androidComponentRowAction() =
        component("android.component.row.action") { RowInCard { AndroidRow(title = "Sync", action = true, onClick = {}) } }

    @Test
    fun androidComponentRowActionThatAdds() =
        component("android.component.row.action-that-adds") {
            RowInCard { AndroidRow(title = "Sync", action = true, leadingIcon = SymbolIcons.Add, onClick = {}) }
        }

    @Test
    fun androidComponentRowDestructiveAction() =
        component("android.component.row.destructive-action") {
            RowInCard { AndroidRow(title = "Sync", action = true, destructive = true, onClick = {}) }
        }

    @Test
    fun androidComponentRowSelected() =
        component("android.component.row.selected") { RowInCard { AndroidRow(title = "Sync", selected = true) } }

    @Test
    fun androidComponentRowNested() =
        component("android.component.row.nested") {
            RowInCard {
                AndroidRow(title = "Pause between sides and discs", switch = RowSwitch(true) {})
                AndroidRow(title = "Countdown", subtitle = "Off", nested = true)
            }
        }

    @Test
    fun androidComponentRowToneWarning() =
        component("android.component.row.tone-warning") {
            RowInCard { AndroidRow(title = "Sync", subtitle = "Synced", tone = StatusTone.WARNING) }
        }

    @Test
    fun androidComponentRowToneDanger() =
        component("android.component.row.tone-danger") {
            RowInCard { AndroidRow(title = "Sync", subtitle = "Synced", tone = StatusTone.DANGER) }
        }

    @Test
    fun androidComponentRowButton() = component("android.component.row.button") { RowInCard { AndroidRow(title = "Sync", onClick = {}) } }

    @Test
    fun androidComponentRowOff() = component("android.component.row.off") { RowInCard { AndroidRow(title = "Sync", enabled = false) } }

    @Test
    fun androidComponentRowPlaceMiddle() =
        component("android.component.row.place-middle") {
            AndroidRow(
                title = "Sync",
                modifier = Modifier.background(MaterialTheme.colorScheme.surfaceContainer, RoundedCornerShape(4.dp)),
            )
        }

    @Test
    fun androidComponentRowNoPlace() = component("android.component.row.no-place") { AndroidRow(title = "Sync") }
}

/** A row in the card every row of a list is in, set in from the preview's edges as the design system's card is. */
@Composable
private fun RowInCard(content: @Composable () -> Unit) =
    AndroidCard(
        modifier = Modifier.padding(start = ThemeSpace.edge, top = ThemeSpace.related, end = ThemeSpace.edge),
    ) { content() }
