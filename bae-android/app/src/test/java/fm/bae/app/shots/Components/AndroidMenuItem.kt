package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import fm.bae.app.ui.library.AndroidMenuItem
import fm.bae.app.ui.library.GroupByArtistItem
import org.junit.Test

/** The component shots of `AndroidMenuItem`, one for each row of its preview. */
class AndroidMenuItemShots : BoardShot() {
    @Test
    fun androidComponentMenuItemDefault() =
        component("android.component.menu-item.default") {
            // The menu's own window isn't drawn in a test: its surface is drawn here.
            Surface(color = MenuDefaults.containerColor, shape = MenuDefaults.shape) {
                Column {
                    AndroidMenuItem("Albums", selected = true, onClick = {})
                    AndroidMenuItem("Artists", selected = false, onClick = {})
                    AndroidMenuItem("Composers (pressed)", selected = false, onClick = {})
                    GroupByArtistItem(grouped = false, onChange = {})
                    GroupByArtistItem(grouped = true, onChange = {})
                }
            }
        }
}
