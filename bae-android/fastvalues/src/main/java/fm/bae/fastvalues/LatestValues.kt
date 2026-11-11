package fm.bae.fastvalues

import android.os.Handler
import android.os.Looper
import androidx.annotation.VisibleForTesting
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeBootstrapStep
import uniffi.bae_bridge.BridgeClosingStep
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeFastValue
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeLibraryOpening
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeSetupProgress
import uniffi.bae_bridge.BridgeStorageSetupProgress
import uniffi.bae_bridge.BridgeTrackBuffered
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.KeyedValueStream
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap

// The one place the app touches the bridge's KeyedValueStream: values core
// sends many times a second (transfer bytes and rates, the playing track's
// position, a library open's steps, where a restore or join stands) arrive in
// a stream the bridge holds and writes itself. The app pulls which keys
// started or stopped holding a value, through a ScreenSubscription, and tells
// the Views watching those keys. FrameFollowing hands a value to
// the View drawing that key once per display frame, whether the value moves
// every frame or only now and then, such as where a join stands. App code
// passes streams and sources around; it holds no value of its own.
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
            fed?.latest(key.encodeToByteArray())?.let { value ->
                checkNotNull(read(value)) { "a stream was fed $value" }
            }
        }

    /**
     * Whether [key] holds a value that is changing; a key at rest is read when a pull names it. A
     * preview's or a test's values change only by [write], which tells the Views watching the key,
     * so they are always at rest.
     */
    internal fun moving(key: String): Boolean =
        if (preview != null) {
            false
        } else {
            fed?.moving(key.encodeToByteArray()) ?: false
        }

    /**
     * A test's write of [key] into a stream [written] made, telling the Views
     * watching it on the main thread when its value changes.
     * App code never writes a stream; only core does.
     */
    @VisibleForTesting
    fun write(
        key: String,
        value: V?,
    ) {
        val values = checkNotNull(preview) { "only a written stream is written" }
        val held = if (value == null) values.remove(key) else values.put(key, value)
        if (held != value) mainThread.post { activity.keysChanged(listOf(key.encodeToByteArray())) }
    }

    /**
     * Hold what [open] starts feeding, pulling which keys started or stopped
     * in [scope] until the returned feed is cancelled. [onError] hears each
     * failed read on the main thread, and the pulls go on.
     */
    internal fun fed(
        scope: CoroutineScope,
        onError: (BridgeException) -> Unit,
        open: () -> KeyedValueStream,
    ): LatestFeed {
        check(preview == null && fed == null) { "a stream is fed twice" }
        val stream = open()
        fed = stream
        val pulls = ScreenSubscription(pulled(stream::next, stream::cancel), scope, activity::keysChanged, onError)
        return LatestFeed(pulls)
    }

    companion object {
        /** A stream a test writes through [write], in place of core. */
        @VisibleForTesting
        fun <V : Any> written(): LatestStream<V> = LatestStream(emptyMap()) { null }

        /** Each downloading release's bytes and rate, by release id; a preview's holds [values], fed by nothing. */
        fun downloads(values: Map<String, BridgeDownloadTransferProgress>? = null) =
            LatestStream(values) { (it as? BridgeFastValue.Download)?.progress }

        /** Where the playing track is, by track id; a preview's holds [values] and nothing feeds it. */
        fun trackPositions(values: Map<String, BridgeTrackPosition>? = null) =
            LatestStream(values) { (it as? BridgeFastValue.TrackPosition)?.position }

        /**
         * The share of each Activity transfer line's batch bytes done, from 0 to 1, by line
         * (`uploads` or `pins`) while its batch holds a release; a preview's holds [values] and
         * nothing feeds it.
         */
        fun activityBars(values: Map<String, Double>? = null): LatestStream<Double> =
            LatestStream(values) { (it as? BridgeFastValue.BatchShare)?.fraction }

        /**
         * The parts of the playing track whose bytes this device holds, by track id; a preview's
         * holds [values] and nothing feeds it.
         */
        fun trackBuffered(values: Map<String, BridgeTrackBuffered>? = null) =
            LatestStream(values) { (it as? BridgeFastValue.TrackBuffered)?.buffered }

        /**
         * True under the playing track's id while its load waits on storage; a preview's holds
         * true under each of [waiting] and nothing feeds it.
         */
        fun trackStorageWaits(waiting: Set<String>? = null): LatestStream<Boolean> =
            LatestStream(waiting?.associateWith { true }) {
                if (it is BridgeFastValue.TrackWaitsOnStorage) true else null
            }

        /**
         * The counted step a restore, join or reset runs, opening or loading, while it runs one; a preview's holds
         * [value], fed by nothing.
         */
        fun bootstrapSteps(value: BridgeBootstrapStep? = null): LatestStream<BridgeBootstrapStep> =
            LatestStream(value?.let { mapOf(ONLY_KEY to it) }) {
                (it as? BridgeFastValue.BootstrapStep)?.step
            }

        /** How far a library's open has come, while it runs a step; a preview's holds [value], fed by nothing. */
        fun openProgress(value: BridgeOpenProgress? = null): LatestStream<BridgeOpenProgress> =
            LatestStream(value?.let { mapOf(ONLY_KEY to it) }) {
                (it as? BridgeFastValue.OpenProgress)?.progress
            }

        /** How far a setup call has come, while it runs a step; a preview's holds [value], fed by nothing. */
        fun setupProgress(value: BridgeSetupProgress? = null): LatestStream<BridgeSetupProgress> =
            LatestStream(value?.let { mapOf(ONLY_KEY to it) }) {
                (it as? BridgeFastValue.SetupProgress)?.progress
            }

        /** How far a storage setup has come, while it runs a step; a preview's holds [value], fed by nothing. */
        fun storageSetupProgress(value: BridgeStorageSetupProgress? = null): LatestStream<BridgeStorageSetupProgress> =
            LatestStream(value?.let { mapOf(ONLY_KEY to it) }) {
                (it as? BridgeFastValue.StorageSetupProgress)?.progress
            }

        /** The step a library's close runs, while it runs one; a preview's holds [value], fed by nothing. */
        fun closingSteps(value: BridgeClosingStep? = null): LatestStream<BridgeClosingStep> =
            LatestStream(value?.let { mapOf(ONLY_KEY to it) }) {
                (it as? BridgeFastValue.ClosingStep)?.step
            }
    }
}

/** The key a stream that holds one value holds it under. */
private const val ONLY_KEY = ""

/** The value of a stream that holds one, for the View that draws it. */
val <V : Any> LatestStream<V>.only: LatestSource<V> get() = source(ONLY_KEY)

// Each feed is pulled in the scope it is handed, the one it lives in. A
// stream's failed read is a bug, which its caller reports through `reportBug`.

/** Fill from core. */
@JvmName("feedDownloadTransfers")
fun LatestStream<BridgeDownloadTransferProgress>.feed(
    appHandle: AppHandleInterface,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, appHandle::feedDownloadTransfers)

/** Fill with the steps of the open [opening] is handed to. */
@JvmName("feedOpenProgress")
fun LatestStream<BridgeOpenProgress>.feed(
    opening: BridgeLibraryOpening,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, opening::feedOpenSteps)

/** Fill with the steps of the close [closing] is handed to. */
@JvmName("feedClosingSteps")
fun LatestStream<BridgeClosingStep>.feed(
    closing: BridgeLibraryClosing,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, closing::feedClosingSteps)

/** Fill with the steps of the setup call [setup] is handed to. */
@JvmName("feedSetupProgress")
fun LatestStream<BridgeSetupProgress>.feed(
    setup: BridgeLibrarySetup,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, setup::feedSetupSteps)

/** Fill with the steps of the storage setup [setup] is handed to. */
@JvmName("feedStorageSetupProgress")
fun LatestStream<BridgeStorageSetupProgress>.feed(
    setup: BridgeLibrarySetup,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, setup::feedSetupSteps)

/** Fill with the counted steps of the restore, join or reset [setup] is handed to. */
@JvmName("feedBootstrapSteps")
fun LatestStream<BridgeBootstrapStep>.feed(
    setup: BridgeLibrarySetup,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, setup::feedSetupSteps)

/** Fill from core. */
@JvmName("feedTrackPositions")
fun LatestStream<BridgeTrackPosition>.feed(
    appHandle: AppHandleInterface,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, appHandle::feedPlaybackPositions)

/** Fill from core. */
@JvmName("feedActivityBars")
fun LatestStream<Double>.feed(
    appHandle: AppHandleInterface,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, appHandle::feedActivityBars)

/** Fill from core. */
@JvmName("feedTrackBuffered")
fun LatestStream<BridgeTrackBuffered>.feed(
    appHandle: AppHandleInterface,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, appHandle::feedTrackBuffered)

/** Fill from core. */
@JvmName("feedTrackStorageWaits")
fun LatestStream<Boolean>.feed(
    appHandle: AppHandleInterface,
    scope: CoroutineScope,
    reportBug: (BridgeException) -> Unit,
): LatestFeed = fed(scope, reportBug, appHandle::feedTrackStorageWaits)

/** A fed stream's feed, which stops when cancelled; the stream then holds nothing. */
class LatestFeed internal constructor(
    private val pulls: ScreenSubscription<List<ByteArray>>,
) {
    /** Stop pulling and cancel the stream. */
    fun cancel() = pulls.stop()
}

/** The main thread, where a written stream tells the Views watching a key. */
private val mainThread by lazy { Handler(Looper.getMainLooper()) }

/** Which keys started or stopped holding a value, told to the Views watching each on the main thread. */
internal class KeyActivity {
    private val watchers = ConcurrentHashMap<String, MutableSet<() -> Unit>>()

    /**
     * Run [changed] on the main thread each time [key] starts or stops holding a value or moving, or
     * changes its value at rest, until [stopWatching].
     */
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

    /**
     * Tell the Views watching the keys that changed; called on the main thread. The bridge
     * names a key by its bytes; every key this app watches is an id, whose bytes are its UTF-8.
     */
    fun keysChanged(keys: List<ByteArray>) {
        keys.flatMap { watchers[it.decodeToString()]?.toList().orEmpty() }.forEach { it() }
    }
}
