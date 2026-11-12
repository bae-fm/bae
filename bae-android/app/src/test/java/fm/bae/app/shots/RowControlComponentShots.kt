package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidInlineIconButton
import fm.bae.app.ui.components.AndroidMoreMenu
import org.junit.Test

/** The component shots of the controls at the end of a row: the icon level with a line of text and the ⋮ with its menu. */
class RowControlComponentShots : BoardShot() {
    @Test
    fun androidComponentInlineIconButtonBesideALine() =
        component("android.component.inline-icon-button.beside-a-line", wide = true) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Low Tide Hours", style = ThemeText.rowTitle.style, modifier = Modifier.weight(1f))
                AndroidInlineIconButton(Icons.Filled.Close, "Cancel", onClick = {})
            }
        }

    @Test
    fun androidComponentMoreMenuClosed() =
        component("android.component.more-menu.closed", wide = true) {
            Box(modifier = Modifier.fillMaxWidth()) {
                Box(modifier = Modifier.align(Alignment.TopEnd)) { AndroidMoreMenu {} }
            }
        }

    /** The menu's contents in a surface like its own, under the ⋮: layoutlib draws an open menu's popup blank. */
    @Test
    fun androidComponentMoreMenuOpen() =
        component("android.component.more-menu.open", wide = true) {
            Box(modifier = Modifier.fillMaxWidth().height(OPEN_MENU_HEIGHT)) {
                Box(modifier = Modifier.align(Alignment.TopEnd)) { AndroidMoreMenu {} }
                Surface(
                    shape = MenuDefaults.shape,
                    color = MenuDefaults.containerColor,
                    tonalElevation = MenuDefaults.TonalElevation,
                    shadowElevation = MenuDefaults.ShadowElevation,
                    modifier = Modifier.align(Alignment.TopEnd).offset(y = MENU_TOP),
                ) {
                    Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(IntrinsicSize.Max)) {
                        DropdownMenuItem(text = { Text("Details…") }, onClick = {})
                        DropdownMenuItem(text = { Text("Rename…") }, onClick = {})
                        HorizontalDivider(modifier = Modifier.padding(vertical = ThemeSpace.related))
                        DropdownMenuItem(text = { Text("Remove from this phone…") }, onClick = {})
                    }
                }
            }
        }
}

private val OPEN_MENU_HEIGHT = 200.dp
private val MENU_TOP = 44.dp
