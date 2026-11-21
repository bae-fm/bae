package fm.bae.app.shots

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Search
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.IconTone
import org.junit.Test

/** The component shots of `AndroidIconButton`, one for each row of its preview. */
class AndroidIconButtonShots : BoardShot() {
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
    fun androidComponentIconButtonOnTheBackdrop() =
        component("android.component.icon-button.on-the-backdrop") {
            OnTheBackdrop { AndroidIconButton(Icons.Filled.Close, "Close", onClick = {}, tone = IconTone.ON_FILL) }
        }
}
