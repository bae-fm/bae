package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeUploadQueueWait
import uniffi.bae_bridge.bridgeQueueWaitKey
import uniffi.bae_bridge.bridgeUploadQueueWaitKey
import java.text.DateFormat
import java.util.Date

/** The words for what holds a transfer queue back ("Paused", "Waiting for network"), from core's catalog. */
fun Context.queueWaitText(wait: BridgeQueueWait): String = coreString(bridgeQueueWaitKey(wait))

/**
 * The words for what holds the upload queue back ("Paused", "Waiting for Wi-Fi", "Retrying at 6:40 PM"), from
 * core's catalog; a retry's clock time is in the person's locale.
 */
fun Context.uploadQueueWaitText(wait: BridgeUploadQueueWait): String {
    val key = bridgeUploadQueueWaitKey(wait)
    return when (wait) {
        is BridgeUploadQueueWait.Retrying -> {
            coreString(key, mapOf("time" to DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(wait.atMs))))
        }

        BridgeUploadQueueWait.Paused, BridgeUploadQueueWait.Offline, BridgeUploadQueueWait.Metered -> {
            coreString(key)
        }
    }
}
