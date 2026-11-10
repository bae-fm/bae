package fm.bae.app.ui.sync

import android.content.Context
import fm.bae.app.DisplayError
import fm.bae.app.displayError
import fm.bae.app.queueWaitText
import fm.bae.app.uploadQueueWaitText
import uniffi.bae_bridge.BridgeSyncActivity

/**
 * The words of [BridgeSyncActivity] that core's catalog and error lines make: sync's failure as the
 * person reads it (nothing for a cancellation), and what holds each queue back. They resolve through
 * the bridge, so the list is handed them rather than making them.
 */
internal data class ActivityWords(
    val failure: DisplayError?,
    val uploadsWait: String?,
    val pinsWait: String?,
)

internal fun activityWords(
    context: Context,
    activity: BridgeSyncActivity?,
) = ActivityWords(
    failure = activity?.failure?.let { context.displayError(it) },
    uploadsWait = activity?.uploads?.wait?.let { context.uploadQueueWaitText(it) },
    pinsWait = activity?.pins?.wait?.let { context.queueWaitText(it) },
)
