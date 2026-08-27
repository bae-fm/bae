package fm.bae.app

import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.AppHandleInterface

private val syncRecoveryLogger = BaeLogger("bae.SyncRecovery")

/**
 * Reconnect the configured provider after a sync failure. Core records any
 * failed reconnect in the sync-status stream, so the existing failure surface
 * remains the one place that reports the result.
 */
internal suspend fun reconnectFailedSync(handle: AppHandleInterface) {
    try {
        handle.reconnectSync()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        syncRecoveryLogger.error("Sync reconnect failed", e)
    }
}
