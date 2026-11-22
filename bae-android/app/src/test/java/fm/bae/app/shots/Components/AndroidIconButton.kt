package fm.bae.app.shots

import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.IconTone
import org.junit.Test

/** The component shots of `AndroidIconButton`, one for each row of its preview. */
class AndroidIconButtonShots : BoardShot() {
    @Test
    fun androidComponentIconButtonBack() =
        component("android.component.icon-button.back") { AndroidIconButton(SymbolIcons.ArrowBack, "Back", onClick = {}) }

    @Test
    fun androidComponentIconButtonSearch() =
        component("android.component.icon-button.search") { AndroidIconButton(SymbolIcons.Search, "Search", onClick = {}) }

    @Test
    fun androidComponentIconButtonMore() =
        component("android.component.icon-button.more") { AndroidIconButton(SymbolIcons.MoreVert, "More", onClick = {}) }

    @Test
    fun androidComponentIconButtonOnTheBackdrop() =
        component("android.component.icon-button.on-the-backdrop") {
            OnTheBackdrop { AndroidIconButton(SymbolIcons.Close, "Close", onClick = {}, tone = IconTone.ON_FILL) }
        }

    @Test
    fun androidComponentIconButtonMenu() =
        component("android.component.icon-button.menu") { AndroidIconButton(SymbolIcons.ArrowBack, "Back", onClick = {}) }

    @Test
    fun androidComponentIconButtonOff() =
        component("android.component.icon-button.off") {
            AndroidIconButton(SymbolIcons.ArrowBack, "Back", onClick = {}, enabled = false)
        }

    @Test
    fun androidComponentIconButtonTonal() =
        component("android.component.icon-button.tonal") {
            AndroidIconButton(SymbolIcons.ArrowBack, "Back", onClick = {}, tonal = true)
        }

    @Test
    fun androidComponentIconButtonToneAccent() =
        component("android.component.icon-button.tone-accent") {
            AndroidIconButton(SymbolIcons.ArrowBack, "Back", onClick = {}, tone = IconTone.ACCENT)
        }

    @Test
    fun androidComponentIconButtonToneMuted() =
        component("android.component.icon-button.tone-muted") {
            AndroidIconButton(SymbolIcons.ArrowBack, "Back", onClick = {}, tone = IconTone.MUTED)
        }

    @Test
    fun androidComponentIconButtonToneWarning() =
        component("android.component.icon-button.tone-warning") {
            AndroidIconButton(SymbolIcons.ArrowBack, "Back", onClick = {}, tone = IconTone.WARNING)
        }
}
