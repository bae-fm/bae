package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidUsageKey
import fm.bae.app.ui.components.StorageUsageSegment
import org.junit.Test
import uniffi.bae_bridge.BridgeStorageKind

/** The component shots of `AndroidUsageKey`, one for each row of its preview. */
class AndroidUsageKeyShots : BoardShot() {
    @Test
    fun androidComponentUsageKeyPinned() =
        component("android.component.usage-key.pinned") { AndroidUsageKey(key(BridgeStorageKind.PINNED, "Pinned", "6.1 GB")) }

    @Test
    fun androidComponentUsageKeyCache() =
        component("android.component.usage-key.cache") { AndroidUsageKey(key(BridgeStorageKind.CACHE, "Cache", "1.8 GB")) }

    @Test
    fun androidComponentUsageKeyLibrary() =
        component("android.component.usage-key.library") { AndroidUsageKey(key(BridgeStorageKind.LIBRARY, "Library", "420 MB")) }

    @Test
    fun androidComponentUsageKeyFromBeforeTheReset() =
        component("android.component.usage-key.from-before-the-reset") {
            AndroidUsageKey(key(BridgeStorageKind.BEFORE_RESET, "From before the reset", "960 MB"))
        }
}

private fun key(
    kind: BridgeStorageKind,
    label: String,
    size: String,
) = StorageUsageSegment(kind, label, size, 0f)
