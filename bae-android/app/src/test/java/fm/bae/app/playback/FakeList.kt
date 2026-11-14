package fm.bae.app.playback

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeListCountExtra
import uniffi.bae_bridge.BridgeListPosition
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgeListWindow
import uniffi.bae_bridge.BridgeListWindowValue
import uniffi.bae_bridge.BridgeSortValue
import uniffi.bae_bridge.ListCountSubscription
import uniffi.bae_bridge.ListPositionSubscription
import uniffi.bae_bridge.ListWindowSubscription
import uniffi.bae_bridge.NoHandle

/**
 * A list as the bridge lists it, over [rows] in the order of their ids: a key is a row's id, a
 * window answers the range of ids it names, and every window and count the handle opened
 * is kept so a test can change the list and deliver the change.
 */
internal open class FakeList<Row>(
    var rows: List<Row>,
    private val idOf: (Row) -> String,
    private val wrap: (List<Row>) -> BridgeListRows,
    private val extra: (Int) -> BridgeListCountExtra = { BridgeListCountExtra.None },
    /** Whether a window or a count answers as soon as it is opened; otherwise a test delivers with [changed]. */
    var deliverImmediately: Boolean = true,
    /** The failure each window answers with instead of its rows. */
    var windowFailure: BridgeException? = null,
) {
    val windows = mutableListOf<FakeListWindowSubscription>()
    val counts = mutableListOf<FakeListCountSubscription>()
    val positions = mutableListOf<FakeListPositionSubscription>()

    /** The first count the handle opened, once it has. */
    val firstCount = CompletableDeferred<FakeListCountSubscription>()

    /** The first window the handle opened, once it has. */
    val firstWindow = CompletableDeferred<FakeListWindowSubscription>()

    /** The key of the row at [position] of the list as it stands: its id, which the list is in the order of. */
    fun key(position: Int): List<BridgeSortValue> = keyOf(idOf(rows[position]))

    fun keyOf(id: String): List<BridgeSortValue> = listOf(BridgeSortValue.Text(id))

    private fun id(key: List<BridgeSortValue>): String = (key.single() as BridgeSortValue.Text).value

    /** The position of the first row whose id is not before [key]'s: where a row with that key is or would be. */
    private fun seek(key: List<BridgeSortValue>): Int = rows.indexOfFirst { idOf(it) >= id(key) }.let { if (it < 0) rows.size else it }

    fun keyAt(position: ULong): List<BridgeSortValue>? = if (rows.isEmpty()) null else key(minOf(position.toInt(), rows.size - 1))

    fun count() = BridgeListCount(rows.size.toULong(), extra(rows.size))

    fun value(window: BridgeListWindow): BridgeListWindowValue {
        val held: List<Row>
        val first: Int?
        val beyond: Int?
        when (window) {
            is BridgeListWindow.Forward -> {
                val start = window.from?.let(::seek) ?: 0
                val end = window.until?.let(::seek) ?: rows.size
                val stop = minOf(start + window.limit.toInt(), end)
                held = if (start < stop) rows.subList(start, stop) else emptyList()
                first = start.takeIf { held.isNotEmpty() }
                beyond = stop.takeIf { it < end }
            }

            is BridgeListWindow.Backward -> {
                val end = seek(window.until)
                val start = maxOf(0, end - window.limit.toInt())
                held = if (start < end) rows.subList(start, end) else emptyList()
                first = start.takeIf { held.isNotEmpty() }
                beyond = (start - 1).takeIf { start > 0 }
            }
        }
        return BridgeListWindowValue(
            window,
            wrap(held),
            first?.let(::key),
            beyond?.let(::key),
        )
    }

    /** Deliver the list as it stands to every count, window and position open, the count first. */
    fun changed() {
        counts.toList().forEach { it.deliver() }
        windows.toList().forEach { it.deliver() }
        positions.toList().forEach { it.deliver() }
    }

    /** Deliver the count alone, as a write that adds or removes a row does beside the windows. */
    fun emitCount() = counts.toList().forEach { it.deliver() }

    fun openWindow(window: BridgeListWindow): FakeListWindowSubscription =
        FakeListWindowSubscription(this, window).also {
            windows += it
            firstWindow.complete(it)
            if (deliverImmediately) it.deliver()
        }

    fun openCount(): FakeListCountSubscription =
        FakeListCountSubscription(this).also {
            counts += it
            firstCount.complete(it)
            if (deliverImmediately) it.deliver()
        }

    fun openPosition(key: List<BridgeSortValue>): FakeListPositionSubscription =
        FakeListPositionSubscription(this, key).also {
            positions += it
            if (deliverImmediately) it.deliver()
        }

    /** How many rows come before the row at [key]. */
    fun rowsBefore(key: List<BridgeSortValue>): ULong = seek(key).toULong()
}

internal class FakeListWindowSubscription(
    private val list: FakeList<*>,
    window: BridgeListWindow,
) : ListWindowSubscription(NoHandle) {
    private val events = Channel<Result<BridgeListWindowValue>>(Channel.UNLIMITED)

    /** Each window this subscription was asked to read, the first one it opened with. */
    val requested = mutableListOf(window)
    val window: BridgeListWindow get() = requested.last()
    var cancelled = false
        private set

    /** Completes when the subscription is cancelled. */
    val cancelledSignal = CompletableDeferred<Unit>()

    override fun setWindow(window: BridgeListWindow) {
        requested += window
        deliver()
    }

    override suspend fun next(): BridgeListWindowValue = events.receive().getOrThrow()

    override suspend fun cancel() {
        cancelled = true
        cancelledSignal.complete(Unit)
    }

    fun deliver() {
        events.trySend(list.windowFailure?.let { Result.failure(it) } ?: Result.success(list.value(window)))
    }

    /** Deliver [value] as it is, whatever the list holds. */
    fun deliver(value: BridgeListWindowValue) {
        events.trySend(Result.success(value))
    }
}

internal class FakeListCountSubscription(
    private val list: FakeList<*>,
) : ListCountSubscription(NoHandle) {
    private val events = Channel<Result<BridgeListCount>>(Channel.UNLIMITED)
    var cancelled = false
        private set

    /** Completes when the subscription is cancelled. */
    val cancelledSignal = CompletableDeferred<Unit>()

    override suspend fun next(): BridgeListCount = events.receive().getOrThrow()

    override suspend fun cancel() {
        cancelled = true
        cancelledSignal.complete(Unit)
    }

    fun deliver() {
        events.trySend(Result.success(list.count()))
    }
}

internal class FakeListPositionSubscription(
    private val list: FakeList<*>,
    key: List<BridgeSortValue>,
) : ListPositionSubscription(NoHandle) {
    private val events = Channel<Result<BridgeListPosition>>(Channel.UNLIMITED)
    val keys = mutableListOf(key)

    override fun setKey(key: List<BridgeSortValue>) {
        keys += key
        deliver()
    }

    override suspend fun next(): BridgeListPosition = events.receive().getOrThrow()

    override suspend fun cancel() {}

    fun deliver() {
        val key = keys.last()
        events.trySend(Result.success(BridgeListPosition(key, list.rowsBefore(key))))
    }
}

/** The album grid as the bridge lists it, over [albums] in the order of their ids. */
internal class FakeAlbumList(
    albums: List<BridgeAlbum> = emptyList(),
    deliverImmediately: Boolean = true,
    windowFailure: BridgeException? = null,
) : FakeList<BridgeAlbum>(
        albums,
        idOf = { it.id },
        wrap = { held -> BridgeListRows.Albums(held.map { BridgeAlbumAppearance(album = it, creditedWith = null) }) },
        extra = { BridgeListCountExtra.AlbumGrid(it.toULong()) },
        deliverImmediately = deliverImmediately,
        windowFailure = windowFailure,
    ) {
    var albums: List<BridgeAlbum>
        get() = rows
        set(value) {
            rows = value
        }
}

/** A composer's page as the bridge lists it, over [rows] in the order of their numbers as text. */
internal class FakeComposerPage(
    rows: List<BridgeComposerDetailRow> = emptyList(),
) : FakeList<BridgeComposerDetailRow>(
        rows,
        idOf = { it.id.toString() },
        wrap = { BridgeListRows.ComposerPage(it) },
    )

/** The composer list as the bridge lists it, over [rows] in the order of their artist ids. */
internal class FakeComposerList(
    rows: List<BridgeComposerSummary> = emptyList(),
) : FakeList<BridgeComposerSummary>(
        rows,
        idOf = { it.artistId },
        wrap = { BridgeListRows.Composers(it) },
    )
