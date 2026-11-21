package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidMoreMenu
import org.junit.Test

/** The component shots of `AndroidMoreMenu`, one for each row of its preview. */
class AndroidMoreMenuShots : BoardShot() {
    @Test
    fun androidComponentMoreMenuClosed() =
        component("android.component.more-menu.closed") {
            Box(modifier = Modifier.fillMaxWidth()) {
                Box(modifier = Modifier.align(Alignment.TopEnd)) { AndroidMoreMenu {} }
            }
        }

    /** The menu's contents in a surface like its own, under the ⋮: layoutlib draws an open menu's popup blank. */
    @Test
    fun androidComponentMoreMenuOpen() =
        component("android.component.more-menu.open") {
            Box(modifier = Modifier.fillMaxWidth().height(OPEN_MENU_HEIGHT)) {
                Box(modifier = Modifier.align(Alignment.TopEnd)) { AndroidMoreMenu {} }
                OpenMenu(top = MENU_TOP) {
                    DropdownMenuItem(text = { Text("Details…") }, onClick = {})
                    DropdownMenuItem(text = { Text("Rename…") }, onClick = {})
                    HorizontalDivider(modifier = Modifier.padding(vertical = ThemeSpace.related))
                    DropdownMenuItem(text = { Text("Remove from this phone…") }, onClick = {})
                }
            }
        }
}

private val OPEN_MENU_HEIGHT = 200.dp

private val MENU_TOP = 44.dp
