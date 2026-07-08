package fm.bae.app.ui

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The settings sync row's state derives from the runtime snapshot's error and
 * readiness. A present error is a failed cycle and wins over readiness (offer a
 * reconnect); absent error distinguishes a live loop from one still coming up.
 */
class SettingsSyncStatusTest {
    @Test
    fun errorMapsToDisconnectedCarryingTheMessage() {
        assertEquals(
            SettingsSyncStatus.Disconnected("network unreachable"),
            settingsSyncStatus(syncError = "network unreachable", syncReady = false),
        )
    }

    @Test
    fun errorWinsOverReadiness() {
        assertEquals(
            SettingsSyncStatus.Disconnected("bucket rejected the request"),
            settingsSyncStatus(syncError = "bucket rejected the request", syncReady = true),
        )
    }

    @Test
    fun readyWithoutErrorMapsToSynced() {
        assertEquals(
            SettingsSyncStatus.Synced,
            settingsSyncStatus(syncError = null, syncReady = true),
        )
    }

    @Test
    fun notReadyWithoutErrorMapsToSyncing() {
        assertEquals(
            SettingsSyncStatus.Syncing,
            settingsSyncStatus(syncError = null, syncReady = false),
        )
    }
}
