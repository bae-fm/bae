package fm.bae.app.playback

import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.AddedDeviceSubscription
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeQueueSnapshot
import uniffi.bae_bridge.BridgeRequestedProfile
import uniffi.bae_bridge.DownloadCountsSubscription
import uniffi.bae_bridge.LibraryNameSubscription
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.QueueSubscription
import uniffi.bae_bridge.RequestedProfileSubscription
import uniffi.bae_bridge.UnmeteredOnlySubscription
import uniffi.bae_bridge.UploadsPausedSubscription

/**
 * What a test hands a pulled subscription in place of core: each value or
 * failure answers the next pull, and cancelling answers a waiting pull that
 * it was cancelled.
 */
internal class PullChannel<V> {
    private val answers = Channel<Result<V>>(Channel.UNLIMITED)

    var cancelled = false
        private set

    fun deliver(value: V) {
        answers.trySend(Result.success(value))
    }

    fun fail(error: BridgeException) {
        answers.trySend(Result.failure(error))
    }

    suspend fun next(): V = answers.receive().getOrThrow()

    fun cancel() {
        cancelled = true
        answers.close(BridgeException.Cancelled())
    }
}

/** The library's name a test hands the app. */
internal class FakeLibraryNameSubscription : LibraryNameSubscription(NoHandle) {
    val pulls = PullChannel<String>()

    override suspend fun next(): String = pulls.next()

    override suspend fun cancel() = pulls.cancel()

    override fun close() = pulls.cancel()
}

/** The requested profile a test hands the app. */
internal class FakeRequestedProfileSubscription : RequestedProfileSubscription(NoHandle) {
    val pulls = PullChannel<BridgeRequestedProfile?>()

    override suspend fun next(): BridgeRequestedProfile? = pulls.next()

    override suspend fun cancel() = pulls.cancel()

    override fun close() = pulls.cancel()
}

/** The added device a test hands the app. */
internal class FakeAddedDeviceSubscription : AddedDeviceSubscription(NoHandle) {
    val pulls = PullChannel<BridgeActiveDevice?>()

    override suspend fun next(): BridgeActiveDevice? = pulls.next()

    override suspend fun cancel() = pulls.cancel()

    override fun close() = pulls.cancel()
}

/** A queue nothing delivers: each pull waits until cancelled. */
internal class FakeQueueSubscription : QueueSubscription(NoHandle) {
    private val pulls = PullChannel<BridgeQueueSnapshot>()

    override suspend fun next(): BridgeQueueSnapshot = pulls.next()

    override suspend fun cancel() = pulls.cancel()

    override fun close() = pulls.cancel()
}

/** Download counts nothing delivers: each pull waits until cancelled. */
internal class FakeDownloadCountsSubscription : DownloadCountsSubscription(NoHandle) {
    private val pulls = PullChannel<BridgeDownloadCounts>()

    override suspend fun next(): BridgeDownloadCounts = pulls.next()

    override suspend fun cancel() = pulls.cancel()

    override fun close() = pulls.cancel()
}

/** Whether files move only on unmetered networks, which nothing delivers: each pull waits until cancelled. */
internal class FakeUnmeteredOnlySubscription : UnmeteredOnlySubscription(NoHandle) {
    private val pulls = PullChannel<Boolean>()

    override suspend fun next(): Boolean = pulls.next()

    override suspend fun cancel() = pulls.cancel()

    override fun close() = pulls.cancel()
}

/** Whether uploads are paused, which nothing delivers: each pull waits until cancelled. */
internal class FakeUploadsPausedSubscription : UploadsPausedSubscription(NoHandle) {
    private val pulls = PullChannel<Boolean>()

    override suspend fun next(): Boolean = pulls.next()

    override suspend fun cancel() = pulls.cancel()

    override fun close() = pulls.cancel()
}
