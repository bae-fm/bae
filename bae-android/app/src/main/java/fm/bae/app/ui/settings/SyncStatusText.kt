package fm.bae.app.ui.settings

import android.content.Context
import android.text.format.DateUtils
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.currentLocale
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.components.StatusTone
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncTone
import uniffi.bae_bridge.bridgeSyncStatusKey
import java.text.DateFormat
import java.util.Date

/** The state in the person's language: core's words for it. */
internal fun syncStatusText(
    context: Context,
    shown: BridgeSyncShown,
): String = context.coreString(bridgeSyncStatusKey(shown.status))

/**
 * Why the state is what it is, in core's words. A cause that names when sync resumes takes that time as its last
 * argument, written as the device's locale writes a time.
 */
internal fun syncCauseText(
    context: Context,
    cause: BridgeSyncCause,
): String {
    val args =
        cause.message.args
            .associate { it.name to it.value as Any }
            .toMutableMap()
    cause.resumesAt?.let { moment ->
        args["time"] =
            DateFormat
                .getTimeInstance(DateFormat.SHORT, context.currentLocale())
                .format(Date.from(moment))
    }
    return context.coreString(cause.message.key, args)
}

/** When sync last finished, as "2 hours ago". */
internal fun syncLastSyncedText(
    moment: java.time.Instant,
    now: java.time.Instant = java.time.Instant.now(),
): CharSequence {
    val then = moment.toEpochMilli()
    return DateUtils.getRelativeTimeSpanString(then, now.toEpochMilli(), DateUtils.MINUTE_IN_MILLIS)
}

/** The color a state reads in; none for the ordinary states. */
internal fun BridgeSyncTone.statusTone(): StatusTone? =
    when (this) {
        BridgeSyncTone.NEUTRAL -> null
        BridgeSyncTone.SUCCESS -> StatusTone.SUCCESS
        BridgeSyncTone.WARNING -> StatusTone.WARNING
        BridgeSyncTone.DANGER -> StatusTone.DANGER
    }

/**
 * The words for the sync state a line holds: core's for a state as read, this app's for a read that failed; none
 * before the first read.
 */
internal fun syncLineText(
    context: Context,
    line: SyncLine,
): String? =
    when (line) {
        SyncLine.Checking -> null
        SyncLine.Unreadable -> context.getString(R.string.settings_sync_unreadable)
        is SyncLine.Read -> syncStatusText(context, line.shown)
    }

/** Which provider a location is on. */
internal fun storageProvider(location: BridgeStorageLocation): BridgeCloudProvider =
    when (location) {
        is BridgeStorageLocation.S3 -> BridgeCloudProvider.S3
        is BridgeStorageLocation.Dropbox -> BridgeCloudProvider.DROPBOX
        is BridgeStorageLocation.OneDrive -> BridgeCloudProvider.ONE_DRIVE
        is BridgeStorageLocation.CloudKit -> BridgeCloudProvider.CLOUD_KIT
    }
