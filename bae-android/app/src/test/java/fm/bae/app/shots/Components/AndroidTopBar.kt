package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AlbumMenuActions
import fm.bae.app.ui.albumdetail.AlbumOptionsMenu
import fm.bae.app.ui.albumdetail.PlayMenuActions
import fm.bae.app.ui.components.AndroidTopBar
import fm.bae.app.ui.library.WhoseMenu
import org.junit.Test
import uniffi.bae_bridge.BridgeHistoryShown

/** The component shots of `AndroidTopBar`, one for each row of its preview. */
class AndroidTopBarShots : BoardShot() {
    @Test
    fun androidComponentTopBarBack() = component("android.component.top-bar.back") { AndroidTopBar(onBack = {}, title = "Storage") }

    @Test
    fun androidComponentTopBarMenu() =
        component("android.component.top-bar.menu") {
            AndroidTopBar(
                onBack = {},
                title = "History",
                actions = { WhoseMenu(menu = historyChoice(BridgeHistoryShown.Everyone), open = false, onOpen = {}, onChoose = {}) },
            )
        }

    @Test
    fun androidComponentTopBarLongTitle() =
        component("android.component.top-bar.long-title") {
            AndroidTopBar(onBack = {}, title = "Ines Varga Trio and friends")
        }

    @Test
    fun androidComponentTopBarNoTitle() =
        component("android.component.top-bar.no-title") { AndroidTopBar(onBack = {}, backLabel = "Back to library") }

    @Test
    fun androidComponentTopBarNoTitleWithMenu() =
        component("android.component.top-bar.no-title-with-menu") {
            AndroidTopBar(onBack = {}, backLabel = "Back to library") {
                AlbumOptionsMenu(
                    AlbumMenuActions(
                        play = PlayMenuActions(onShuffle = {}, onPlayNext = {}, onAddToQueue = {}),
                        onRate = {},
                        pin = null,
                        onSetPrimaryRelease = null,
                        onDelete = {},
                    ),
                )
            }
        }
}
