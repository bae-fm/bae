package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeUploadQueueWait
import uniffi.bae_bridge.bridgeQueueWaitKey
import uniffi.bae_bridge.bridgeUploadQueueWaitKey
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/** The words for what holds a transfer queue back ("Paused", "Waiting for network"), from core's catalog. */
fun Context.queueWaitText(wait: BridgeQueueWait): String = coreString(bridgeQueueWaitKey(wait))

/** A retry's clock time ("6:40 PM", "18:40") in [locale]'s short time format, in [zone]. */
internal fun retryTimeText(
    atMs: Long,
    locale: Locale,
    zone: TimeZone = TimeZone.getDefault(),
): String =
    DateFormat
        .getTimeInstance(DateFormat.SHORT, locale)
        .apply { timeZone = zone }
        .format(Date(atMs))

/**
 * The words for what holds the upload queue back ("Paused", "Waiting for Wi-Fi", "Retrying at 6:40 PM"), from
 * core's catalog; a retry's clock time is in the person's locale.
 */
fun Context.uploadQueueWaitText(wait: BridgeUploadQueueWait): String {
    val key = bridgeUploadQueueWaitKey(wait)
    return when (wait) {
        is BridgeUploadQueueWait.Retrying -> {
            coreString(key, mapOf("time" to retryTimeText(wait.atMs, currentLocale())))
        }

        BridgeUploadQueueWait.Paused, BridgeUploadQueueWait.Offline, BridgeUploadQueueWait.Metered -> {
            coreString(key)
        }
    }
}
