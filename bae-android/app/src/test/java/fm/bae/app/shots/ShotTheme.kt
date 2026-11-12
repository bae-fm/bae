package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore
import fm.bae.app.ui.appearance.SurfaceTone

/** The look the design boards draw: dark, Midnight, Pink. */
internal val BOARD_APPEARANCE =
    AppearancePreferences(mode = AppearanceMode.DARK, accent = AccentChoice.PINK, tone = SurfaceTone.MIDNIGHT)

/** [BaeTheme] in [appearance] (the boards' look by default), which nothing a shot taps changes. */
@Composable
internal fun ShotTheme(
    appearance: AppearancePreferences = BOARD_APPEARANCE,
    content: @Composable () -> Unit,
) {
    val store = remember { AppearanceStore(appearance) {} }
    BaeTheme(appearance = store, content = content)
}
