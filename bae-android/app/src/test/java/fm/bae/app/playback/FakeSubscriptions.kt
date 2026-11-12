package fm.bae.app.playback

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.AdminChangesSubscription
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDeviceStorage
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeFastValue
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteEntry
import uniffi.bae_bridge.BridgePlaybackTimeline
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgeQueueAddition
import uniffi.bae_bridge.BridgeRefusedInvite
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.CastReceiversSubscription
import uniffi.bae_bridge.ConfigSubscription
import uniffi.bae_bridge.DeviceStorageSubscription
import uniffi.bae_bridge.InvitesSubscription
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.PlaybackTimelineSubscription
import uniffi.bae_bridge.PlaybackValuesSubscription
import uniffi.bae_bridge.QueueAdditionsSubscription
import uniffi.bae_bridge.SyncActivitySubscription
import uniffi.bae_bridge.SyncStatusSubscription

/** A stream nothing feeds, which holds nothing: each pull waits until it is cancelled. */
internal class FakeKeyedValueStream : KeyedValueStream(NoHandle) {
    private val cancelled = CompletableDeferred<Unit>()

    override fun latest(key: ByteArray): BridgeFastValue? = null

    override fun moving(key: ByteArray): Boolean = false

    override suspend fun next(): List<ByteArray> {
        cancelled.await()
        throw BridgeException.Cancelled()
    }

    override fun cancel() {
        cancelled.complete(Unit)
    }
}

/** A cast receiver list nothing delivers: each pull waits until cancelled. */
internal class FakeCastReceiversSubscription : CastReceiversSubscription(NoHandle) {
    private val receivers = Channel<BridgeCastReceiverList>(Channel.UNLIMITED)

    override suspend fun next(): BridgeCastReceiverList = receivers.receive()

    override suspend fun cancel() {
        receivers.close(BridgeException.Cancelled())
    }

    override fun close() {
        receivers.close(BridgeException.Cancelled())
    }
}

/** A sync status a test hands the app a value at a time: each pull waits for the next. */
internal class FakeSyncStatusSubscription : SyncStatusSubscription(NoHandle) {
    private val values = Channel<BridgeSyncStatusSnapshot>(Channel.UNLIMITED)

    fun deliver(status: BridgeSyncStatusSnapshot) {
        values.trySend(status)
    }

    override suspend fun next(): BridgeSyncStatusSnapshot = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** A sync activity a test hands the app a value at a time: each pull waits for the next. */
internal class FakeSyncActivitySubscription : SyncActivitySubscription(NoHandle) {
    private val values = Channel<BridgeSyncActivity>(Channel.UNLIMITED)

    fun deliver(activity: BridgeSyncActivity) {
        values.trySend(activity)
    }

    override suspend fun next(): BridgeSyncActivity = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Admin changes a test hands the app a list at a time: each pull waits for the next. */
internal class FakeAdminChangesSubscription : AdminChangesSubscription(NoHandle) {
    private val values = Channel<List<BridgeAdminChange>>(Channel.UNLIMITED)

    fun deliver(changes: List<BridgeAdminChange>) {
        values.trySend(changes)
    }

    override suspend fun next(): List<BridgeAdminChange> = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Open invites a test hands the app a list at a time: each pull waits for the next. */
internal class FakeInvitesSubscription : InvitesSubscription(NoHandle) {
    private val values = Channel<List<BridgeInviteEntry>>(Channel.UNLIMITED)

    fun deliver(
        invites: List<BridgeInvite>,
        refused: List<BridgeRefusedInvite> = emptyList(),
    ) {
        values.trySend(invites.map { BridgeInviteEntry.Open(it) } + refused.map { BridgeInviteEntry.Refused(it) })
    }

    override suspend fun next(): List<BridgeInviteEntry> = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Preferences nothing delivers: each pull waits until cancelled. */
internal class FakeConfigSubscription : ConfigSubscription(NoHandle) {
    private val values = Channel<BridgeConfig>(Channel.UNLIMITED)

    override suspend fun next(): BridgeConfig = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Playback values a test hands the app a value at a time: each pull waits for the next. */
internal class FakePlaybackValuesSubscription : PlaybackValuesSubscription(NoHandle) {
    private val values = Channel<BridgePlaybackValues>(Channel.UNLIMITED)

    fun deliver(playback: BridgePlaybackValues) {
        values.trySend(playback)
    }

    override suspend fun next(): BridgePlaybackValues = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Queue additions nothing delivers: each pull waits until cancelled. */
internal class FakeQueueAdditionsSubscription : QueueAdditionsSubscription(NoHandle) {
    private val additions = Channel<BridgeQueueAddition>(Channel.UNLIMITED)

    override suspend fun next(): BridgeQueueAddition = additions.receive()

    override suspend fun cancel() {
        additions.close(BridgeException.Cancelled())
    }

    override fun close() {
        additions.close(BridgeException.Cancelled())
    }
}

/** Timelines a test hands the app one at a time: each pull waits for the next, or answers a failure a test queued. */
internal class FakePlaybackTimelineSubscription : PlaybackTimelineSubscription(NoHandle) {
    private val pulls = Channel<Result<BridgePlaybackTimeline>>(Channel.UNLIMITED)

    fun deliver(timeline: BridgePlaybackTimeline) {
        pulls.trySend(Result.success(timeline))
    }

    /** The next pull fails with [error]; the pull after it waits again. */
    fun fail(error: BridgeException) {
        pulls.trySend(Result.failure(error))
    }

    override suspend fun next(): BridgePlaybackTimeline = pulls.receive().getOrThrow()

    override suspend fun cancel() {
        pulls.close(BridgeException.Cancelled())
    }

    override fun close() {
        pulls.close(BridgeException.Cancelled())
    }
}

/** What the phone keeps, a value or a failed read at a time a test hands the app: each pull waits for the next. */
internal class FakeDeviceStorageSubscription : DeviceStorageSubscription(NoHandle) {
    private val pulls = Channel<Result<BridgeDeviceStorage>>(Channel.UNLIMITED)

    fun deliver(storage: BridgeDeviceStorage) {
        pulls.trySend(Result.success(storage))
    }

    /** The next pull fails with [error]; the pull after it waits again. */
    fun fail(error: BridgeException) {
        pulls.trySend(Result.failure(error))
    }

    override suspend fun next(): BridgeDeviceStorage = pulls.receive().getOrThrow()

    override suspend fun cancel() {
        pulls.close(BridgeException.Cancelled())
    }

    override fun close() {
        pulls.close(BridgeException.Cancelled())
    }
}
