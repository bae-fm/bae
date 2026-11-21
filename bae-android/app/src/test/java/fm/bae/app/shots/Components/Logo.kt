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

    @Test
    fun androidComponentLogoNoKind() = component("android.component.logo.no-kind") { Logo(size = 120.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoInline() = component("android.component.logo.inline") { Logo(size = 80.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoInlineRightToLeft() =
        component("android.component.logo.inline-right-to-left") { Logo(size = 80.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoSheep() = component("android.component.logo.sheep") { Logo(size = 120.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoWord() = component("android.component.logo.word") { Logo(size = 120.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoSmall32() = component("android.component.logo.small-32") { Logo(size = 32.dp, bob = false, glow = false) }

    @Test
    fun androidComponentLogoSmall16() = component("android.component.logo.small-16") { Logo(size = 16.dp, bob = false, glow = false) }
}
