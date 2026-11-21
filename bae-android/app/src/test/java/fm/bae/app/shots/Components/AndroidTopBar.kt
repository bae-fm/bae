package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AlbumOptionsMenu
import fm.bae.app.ui.components.AndroidTopBar
import fm.bae.app.ui.library.WhoseMenu
import org.junit.Test
import uniffi.bae_bridge.BridgeHistoryShown

/** The component shots of `AndroidTopBar`, one for each row of its preview. */
class AndroidTopBarShots : BoardShot() {
    @Test
    fun androidComponentTopBarBack() =
        component("android.component.top-bar.back", wide = true) { AndroidTopBar(onBack = {}, title = "Storage") }

    @Test
    fun androidComponentTopBarMenu() =
        component("android.component.top-bar.menu", wide = true) {
            AndroidTopBar(
                onBack = {},
                title = "History",
                actions = { WhoseMenu(menu = historyChoice(BridgeHistoryShown.Everyone), open = false, onOpen = {}, onChoose = {}) },
            )
        }

    @Test
    fun androidComponentTopBarLongTitle() =
        component("android.component.top-bar.long-title", wide = true) {
            AndroidTopBar(onBack = {}, title = "Ines Varga Trio and friends")
        }

    @Test
    fun androidComponentTopBarNoTitle() =
        component("android.component.top-bar.no-title", wide = true) { AndroidTopBar(onBack = {}, backLabel = "Back to library") }

    @Test
    fun androidComponentTopBarNoTitleWithMenu() =
        component("android.component.top-bar.no-title-with-menu", wide = true) {
            AndroidTopBar(onBack = {}, backLabel = "Back to library") {
                AlbumOptionsMenu(onSetPrimaryRelease = null, onDelete = {})
            }
        }
}
