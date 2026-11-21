package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.Logo
import org.junit.Test

/** The component shots of `Logo` on Android, which draws only the pair: the sheep over the word. */
class LogoShots : BoardShot() {
    @Test
    fun androidComponentLogoPair() = component("android.component.logo.pair") { Logo(size = 120.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoPairBobAndGlow() =
        component("android.component.logo.pair-bob-and-glow") { Logo(size = 120.dp, bob = true, glow = true) }
}
