package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.Logo
import org.junit.Test

/**
 * The component shots of `Logo` on Android, which draws only the pair: the sheep over the word. The other forms
 * are marked as drawn for the design only in `.app-build`.
 */
class LogoShots : BoardShot() {
    @Test
    fun androidComponentLogoPair() = component("android.component.logo.pair") { Logo(size = 120.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoPairBobAndGlow() =
        component("android.component.logo.pair-bob-and-glow") { Logo(size = 120.dp, bob = true, glow = true) }

    @Test
    fun androidComponentLogoNoKind() = component("android.component.logo.no-kind") { Logo(size = 120.dp, bob = false, glow = false) }
}
