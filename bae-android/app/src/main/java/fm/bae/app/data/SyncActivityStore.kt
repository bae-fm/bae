package fm.bae.app.data

import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.BridgeSyncActivity

/**
 * Everything sync is doing on this device, as core reports it: the top bar's
 * Activity button and the sheet it opens read it here. The sync-activity value
 * stream is its only writer.
 */
class SyncActivityStore(
    /** Each transfer line's bar: the share of its batch's bytes done, which core's stream fills. */
    val bars: LatestStream<Double> = LatestStream.activityBars(),
) {
    private val mutableActivity = MutableStateFlow<BridgeSyncActivity?>(null)
    val activity: StateFlow<BridgeSyncActivity?> = mutableActivity.asStateFlow()

    /** Why the activity couldn't be read, until it is read again. */
    private val mutableError = MutableStateFlow<DisplayError?>(null)
    val error: StateFlow<DisplayError?> = mutableError.asStateFlow()

    private val phoneRecords = PhonePendingRecords()

    /** Applies [value] without the records of [BridgeSyncActivity.attention] a phone can't have: those are a bug. */
    fun apply(
        value: BridgeSyncActivity,
        bugs: BugReports,
    ) {
        val taken = phoneRecords.take(value.attention, "list the records that need attention", bugs::report)
        mutableActivity.value = value.copy(attention = taken.listed)
        mutableError.value = null
    }

    fun fail(error: DisplayError) {
        mutableError.value = error
    }

    /** The bar of [line], for the View that draws it. */
    fun bar(line: ActivityLine): LatestSource<Double> = bars.source(line.key)
}

/** The transfer lines a phone's Activity sheet draws a bar for, named as core's stream keys them. */
enum class ActivityLine(
    val key: String,
) {
    UPLOADS("uploads"),
    PINS("pins"),
}
