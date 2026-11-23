package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
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

/** The component shots of `AndroidMenuItem`, one for each row of its preview. */
class AndroidMenuItemShots : BoardShot() {
    @Test
    fun androidComponentMenuItemDefault() =
        component("android.component.menu-item.default") {
            MenuSurface {
                AndroidMenuItem("Albums", selected = true, onClick = {})
                AndroidMenuItem("Artists", selected = false, onClick = {})
                Pressed { AndroidMenuItem("Composers (pressed)", selected = false, onClick = {}) }
                GroupByArtistItem(grouped = false, onChange = {})
                GroupByArtistItem(grouped = true, onChange = {})
            }
        }

    @Test
    fun androidComponentMenuItemHeading() =
        component("android.component.menu-item.heading") { MenuSurface { AndroidMenuHeading("Artists") } }

    @Test
    fun androidComponentMenuItemDivider() =
        component("android.component.menu-item.divider") {
            MenuSurface {
                AndroidMenuItem("Artists", selected = false, onClick = {})
                AndroidMenuDivider()
                AndroidMenuItem("Artists", selected = false, onClick = {})
            }
        }
}

/** The menu's own window isn't drawn in a test: its surface is drawn here, as the menu makes it. */
@Composable
private fun MenuSurface(content: @Composable () -> Unit) {
    Box(Modifier.padding(ThemeSpace.related)) {
        Surface(
            shape = AndroidMenuShape,
            color = androidMenuColor(),
            tonalElevation = MenuDefaults.TonalElevation,
            shadowElevation = MenuDefaults.ShadowElevation,
        ) {
            Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(IntrinsicSize.Max)) {
                AndroidMenuItemStyle { content() }
            }
        }
    }
}

/** An item under the finger: Material lays the content colour over it at its pressed opacity, which a test can't press. */
@Composable
private fun Pressed(content: @Composable () -> Unit) {
    Box(Modifier.background(MaterialTheme.colorScheme.onSurface.copy(alpha = PRESSED_OPACITY))) { content() }
}

private const val PRESSED_OPACITY = 0.1f
