package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.BridgeFixtures
import fm.bae.app.ui.settings.AndroidSettingsStorage
import fm.bae.app.ui.settings.deleteBeforeResetParts
import org.junit.Test
import uniffi.bae_bridge.BridgeDeviceStorage

/** The shots of the ready android.settings.storage.* boards: what the phone keeps, and what a reset left. */
class SettingsStorageShots : BoardShot() {
    /** The Storage page with pinned releases, cache and the library's records. */
    @Test
    fun androidSettingsStorage() = shot("android.settings.storage") { ScreenScene { Storage(BridgeFixtures.deviceStorage()) } }

    /** The Storage page while a reset's data is still here, with its deletion under its own heading. */
    @Test
    fun androidSettingsStorageFromBeforeReset() = shot("android.settings.storage.from-before-reset") { ScreenScene { Storage(afterReset) } }

    /** The question before deleting what the reset left, over the page. */
    @Test
    fun androidSettingsStorageDeleteFromBeforeReset() =
        shot("android.settings.storage.delete-from-before-reset") {
            InPlaceDialogScene(
                behind = { Storage(afterReset) },
                dialog = { deleteBeforeResetParts(checkNotNull(afterReset.beforeReset), onConfirm = {}, onDismiss = {}) },
            )
        }
}

private val afterReset = BridgeFixtures.deviceStorage(beforeReset = 540_000_000uL)

@Composable
private fun Storage(storage: BridgeDeviceStorage) =
    AndroidSettingsStorage(
        storage = storage,
        asksBeforeUnpinning = true,
        onBack = {},
        onClearCache = {},
        onKeepUpTo = {},
        onAskBeforeUnpinning = {},
        onDeleteBeforeReset = {},
    )
