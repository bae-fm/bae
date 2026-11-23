package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidMenuDivider
import fm.bae.app.ui.components.AndroidMenuHeading
import fm.bae.app.ui.components.AndroidMenuItemStyle
import fm.bae.app.ui.components.AndroidMenuShape
import fm.bae.app.ui.components.androidMenuColor
import fm.bae.app.ui.library.AndroidMenuItem
import fm.bae.app.ui.library.GroupByArtistItem
import org.junit.Test

/**
 * The component shots of `AndroidMenu`, one for each row of its preview. Layoutlib draws an open menu's popup blank,
 * so the row draws the menu's items on a surface made as the menu makes its own.
 */
class AndroidMenuShots : BoardShot() {
    @Test
    fun androidComponentMenuDefault() =
        component("android.component.menu.default") {
            Box(Modifier.padding(ThemeSpace.related)) {
                Surface(
                    shape = AndroidMenuShape,
                    color = androidMenuColor(),
                    tonalElevation = MenuDefaults.TonalElevation,
                    shadowElevation = MenuDefaults.ShadowElevation,
                ) {
                    Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(IntrinsicSize.Max)) {
                        AndroidMenuItemStyle {
                            AndroidMenuHeading("Sort by")
                            AndroidMenuItem(label = "Title", selected = false, onClick = {})
                            AndroidMenuItem(label = "Date added", selected = true, onClick = {})
                            DropdownMenuItem(text = { Text("Descending") }, onClick = {})
                            GroupByArtistItem(grouped = true, onChange = {})
                            AndroidMenuDivider()
                            DropdownMenuItem(text = { Text("Settings") }, onClick = {})
                        }
                    }
                }
            }
        }
}
