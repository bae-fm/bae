package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore

/** [BaeTheme] in the default appearance, as the root opens it for a person who chose nothing, with no file read. */
@Composable
internal fun TestTheme(content: @Composable () -> Unit) {
    val appearance = remember { AppearanceStore(AppearancePreferences()) {} }
    BaeTheme(appearance, content)
}
