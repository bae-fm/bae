package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.ErrorLines
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncIndicator
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

class SyncStatusStoreTest {
    private val errors =
        object : ErrorLines {
            override fun line(reason: BridgePlaybackErrorReason): String? = null

            override fun line(error: BridgeException): String? = null
        }

    @Test
    fun configValuesCannotOverwriteSyncReadinessTransitions() {
        val config = ConfigStore(BridgeFixtures.config())
        val sync = SyncStatusStore { BridgeSyncIndicator.Idle }

        sync.apply(status(syncReady = true), errors)
        config.setConfig(BridgeFixtures.config())
        assertTrue(sync.snapshot.value?.syncReady == true)

        sync.apply(status(syncReady = false), errors)
        assertFalse(sync.snapshot.value?.syncReady == true)
    }

    private fun status(syncReady: Boolean) =
        BridgeSyncStatusSnapshot(
            error = null,
            blocked = emptyList(),
            lastSyncTime = null,
            syncing = false,
            syncReady = syncReady,
        )
}
