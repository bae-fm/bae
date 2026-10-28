package fm.bae.fastvalues

import android.os.Handler
import android.os.Looper
import androidx.annotation.VisibleForTesting
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeFastValue
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.BridgeUploadTransfer
import uniffi.bae_bridge.KeyActivityCallback
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.StreamFailureCallback
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap

// The one place the app touches the bridge's KeyedValueStream: values core
// sends many times a second (transfer bytes and rates, the playing track's
// position) arrive in a stream the
// bridge holds and writes itself, and the only reader of a value is
// FrameFollowing, which hands it to the View drawing that key once per
// display frame. App code passes streams and sources around; it never holds a
// value, so no Compose state can change with one.
// scripts/keyed-value-stream-boundary.py keeps every other file off the
// bridge's stream.

/**
 * The latest value of each key of one fast stream, for the Views that draw a
 * key. Until it is fed it holds nothing, and a preview's holds fixed values
 * that nothing feeds.
 */
class LatestStream<V : Any> private constructor(
    preview: Map<String, V>?,
    private val read: (BridgeFastValue) -> V?,
) {
    /** A preview's or a test's values, which nothing feeds; null for a stream core feeds. */
    private val preview: MutableMap<String, V>? = preview?.let { ConcurrentHashMap(it) }

    internal val activity = KeyActivity()

    @Volatile private var fed: KeyedValueStream? = null

    /** One key of the stream, for the View that draws it. */
    fun source(key: String): LatestSource<V> = LatestSource(this, key)

    internal fun latest(key: String): V? =
        if (preview != null) {
            preview[key]
        } else {
            fed?.latest(key)?.let { value -> checkNotNull(read(value)) { "a stream was fed $value" } }
        }

    /**
     * A test's write of [key] into a stream [written] made, telling the Views
     * watching it when it starts or stops holding a value. App code never
     * writes a stream; only core does.
     */
    @VisibleForTesting
    fun write(
        key: String,
        value: V?,
    ) {
        val values = checkNotNull(preview) { "only a written stream is written" }
        val held = if (value == null) values.remove(key) else values.put(key, value)
        if ((held == null) != (value == null)) activity.onKeysChanged(listOf(key))
    }

    /** Hold what [open] starts feeding, until the returned feed is cancelled. */
    internal fun fed(open: (KeyActivityCallback) -> KeyedValueStream): LatestFeed {
        check(preview == null && fed == null) { "a stream is fed twice" }
        val stream = open(activity)
        fed = stream
        return LatestFeed(stream)
    }

    companion object {
        /** A stream a test writes through [write], in place of core. */
        @VisibleForTesting
        fun <V : Any> written(): LatestStream<V> = LatestStream(emptyMap()) { null }

        /** The files being sent together, while any is; a preview's holds [value] and nothing feeds it. */
        fun uploadTotal(value: BridgeUploadTransfer? = null): LatestStream<BridgeUploadTransfer> =
            LatestStream(value?.let { mapOf(ONLY_KEY to it) }) {
                (it as? BridgeFastValue.UploadTotal)?.transfer
            }

        /** Each downloading release's bytes and rate, by release id; a preview's holds [values], fed by nothing. */
        fun downloads(values: Map<String, BridgeDownloadTransferProgress>? = null) =
            LatestStream(values) { (it as? BridgeFastValue.Download)?.progress }

        /** Where the playing track is, by track id; a preview's holds [values] and nothing feeds it. */
        fun trackPositions(values: Map<String, BridgeTrackPosition>? = null) =
            LatestStream(values) { (it as? BridgeFastValue.TrackPosition)?.position }
    }
}

/** The key a stream that holds one value holds it under. */
private const val ONLY_KEY = ""

/** The stream's one value, for the View that draws it. */
val LatestStream<BridgeUploadTransfer>.only: LatestSource<BridgeUploadTransfer> get() = source(ONLY_KEY)

/** Fill from core, telling [onError] on the main thread each failed read. */
fun LatestStream<BridgeUploadTransfer>.feed(
    appHandle: AppHandleInterface,
    onError: (BridgeException) -> Unit,
): LatestFeed = fed { appHandle.feedUploadTotalTransfer(it, StreamFailureSink(onError)) }

/** Fill from core. */
fun LatestStream<BridgeDownloadTransferProgress>.feed(appHandle: AppHandleInterface): LatestFeed =
    fed { appHandle.feedDownloadTransfers(it) }

/** Fill from core. */
@JvmName("feedTrackPositions")
fun LatestStream<BridgeTrackPosition>.feed(appHandle: AppHandleInterface) = fed { appHandle.feedPlaybackPositions(it) }

/** A fed stream's feed, which stops when cancelled; the stream then holds nothing. */
class LatestFeed internal constructor(
    private val stream: KeyedValueStream,
) {
    fun cancel() = stream.cancel()
}

/**
 * One key of a fast stream, which a composable hands to the View that draws
 * it. Only FrameFollowing reads its value.
 */
@ConsistentCopyVisibility
data class LatestSource<V : Any> internal constructor(
    internal val stream: LatestStream<V>,
    internal val key: String,
) {
    internal val latest: V? get() = stream.latest(key)
}

/** Which keys started or stopped holding a value, told to the Views watching each on the main thread. */
internal class KeyActivity : KeyActivityCallback {
    private val watchers = ConcurrentHashMap<String, MutableSet<() -> Unit>>()
    private val main by lazy { Handler(Looper.getMainLooper()) }

    /** Run [changed] on the main thread each time [key] starts or stops holding a value, until [stopWatching]. */
    fun watch(
        key: String,
        changed: () -> Unit,
    ) {
        watchers.getOrPut(key) { Collections.newSetFromMap(ConcurrentHashMap()) }.add(changed)
    }

    fun stopWatching(
        key: String,
        changed: () -> Unit,
    ) {
        watchers[key]?.remove(changed)
    }

    /** Post once to the main thread for the keys that started or stopped, for the Views watching them. */
    override fun onKeysChanged(keys: List<String>) {
        val watching = keys.flatMap { watchers[it]?.toList().orEmpty() }
        if (watching.isEmpty()) return
        main.post { watching.forEach { it() } }
    }
}

/** A fed stream's failed read, told on the main thread. */
private class StreamFailureSink(
    private val fail: (BridgeException) -> Unit,
) : StreamFailureCallback {
    private val main by lazy { Handler(Looper.getMainLooper()) }

    override fun onError(error: BridgeException) {
        main.post { fail(error) }
    }
}
