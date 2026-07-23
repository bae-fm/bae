package fm.bae.app.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeSyncIndicator

/**
 * The settings sync row maps core's indicator variant to its rendered state; the
 * precedence itself is core's (bae-core's `sync_indicator_tests`). The error line
 * rides alongside the Error variant, which carries only the fact of an error.
 */
class SettingsSyncStatusTest {
    @Test
    fun errorMapsToDisconnectedCarryingTheMessage() {
        assertEquals(
            SettingsSyncStatus.Disconnected("network unreachable"),
            settingsSyncStatus(BridgeSyncIndicator.Error, syncError = "network unreachable"),
        )
    }

    @Test
    fun syncedMapsToSynced() {
        assertEquals(
            SettingsSyncStatus.Synced,
            settingsSyncStatus(BridgeSyncIndicator.Synced(lastSyncTime = 100L), syncError = null),
        )
    }

    @Test
    fun syncingMapsToSyncing() {
        assertEquals(
            SettingsSyncStatus.Syncing,
            settingsSyncStatus(BridgeSyncIndicator.Syncing, syncError = null),
        )
    }

    @Test
    fun idleMapsToSyncing() {
        assertEquals(
            SettingsSyncStatus.Syncing,
            settingsSyncStatus(BridgeSyncIndicator.Idle, syncError = null),
        )
    }
}
