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
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgeListView
import uniffi.bae_bridge.ListCountSubscription
import uniffi.bae_bridge.ListPageSubscription
import uniffi.bae_bridge.ListViewSubscription
import uniffi.bae_bridge.NoHandle

/**
 * A list as the bridge lists it, over [rows] in the order of their ids. Its view answers the way core's does: it
 * keeps a window of three screens around the rows on screen, starting a screen above them, and holds the window at
 * its first row's id, so rows inserted above it move the window's start and not its rows. Its pages answer the way
 * core's do: the rows at the page's places, none past the list's end. The fake keeps every view, page and count the
 * handle opened so a test can change the list and deliver the change.
 */
internal open class FakeList<Row>(
    var rows: List<Row>,
    private val idOf: (Row) -> String,
    private val wrap: (List<Row>) -> BridgeListRows,
    private val extra: (Int) -> BridgeListCountExtra = { BridgeListCountExtra.None },
    /** Whether a view or a count answers as soon as the handle opens it; otherwise a test delivers with [changed]. */
    var deliverImmediately: Boolean = true,
    /** The failure each view answers with instead of its rows. */
    var windowFailure: BridgeException? = null,
    /** The failure each count answers with instead of the number of rows. */
    var countFailure: BridgeException? = null,
) {
    val views = mutableListOf<FakeListViewSubscription>()
    val pages = mutableListOf<FakeListPageSubscription>()
    val counts = mutableListOf<FakeListCountSubscription>()

    /** The first page the handle opened, once it has. */
    val firstPage = CompletableDeferred<FakeListPageSubscription>()

    /** The first count the handle opened, once it has. */
    val firstCount = CompletableDeferred<FakeListCountSubscription>()

    /** The first view the handle opened, once it has. */
    val firstView = CompletableDeferred<FakeListViewSubscription>()

    fun count() = BridgeListCount(rows.size.toULong(), rows.size.toULong(), extra(rows.size))

    /** The id of the row at [position], where a window placed there starts. */
    fun idAt(position: Int): String? = rows.getOrNull(position)?.let(idOf)

    /** The window's rows as the list stands: [limit] of them from the first whose id is not before [from]. */
    fun value(
        from: String?,
        limit: Int,
    ): BridgeListView {
        val start = from?.let { id -> rows.indexOfFirst { idOf(it) >= id }.let { if (it < 0) rows.size else it } } ?: 0
        val stop = minOf(start + limit, rows.size)
        return BridgeListView(start.toULong(), wrap(if (start < stop) rows.subList(start, stop) else emptyList()))
    }

    /** The page's rows as the list stands: the [limit] rows from [offset], none past the list's end. */
    fun page(
        offset: Int,
        limit: Int,
    ): BridgeListRows = wrap(rows.drop(offset).take(limit))

    /** Deliver the list as it stands to every count, view and page open, the count first. */
    fun changed() {
        counts.toList().forEach { it.deliver() }
        views.toList().forEach { it.deliver() }
        pages.toList().forEach { it.deliver() }
    }

    /** Deliver the count alone, as a write that adds or removes a row does beside the view. */
    fun emitCount() = counts.toList().forEach { it.deliver() }

    fun openView(
        first: ULong,
        last: ULong,
    ): FakeListViewSubscription =
        FakeListViewSubscription(this, first.toInt()..last.toInt()).also {
            views += it
            firstView.complete(it)
            if (deliverImmediately) it.deliver()
        }

    fun openPage(
        offset: ULong,
        limit: ULong,
    ): FakeListPageSubscription =
        FakeListPageSubscription(this, offset.toInt(), limit.toInt()).also {
            pages += it
            firstPage.complete(it)
            if (deliverImmediately) it.deliver()
        }

    fun openCount(): FakeListCountSubscription =
        FakeListCountSubscription(this).also {
            counts += it
            firstCount.complete(it)
            if (deliverImmediately) it.deliver()
        }
}

internal class FakeListViewSubscription(
    private val list: FakeList<*>,
    shown: IntRange,
) : ListViewSubscription(NoHandle) {
    private val events = Channel<Result<BridgeListView>>(Channel.UNLIMITED)

    /** Each range of rows on screen the handle reported, starting with the one it opened with. */
    val reported = mutableListOf(shown)

    /** The id of the window's first row; null at the list's top. */
    private var from: String? = null
    private var limit = 0
    var cancelled = false
        private set

    /** Completes when the handle cancels the subscription. */
    val cancelledSignal = CompletableDeferred<Unit>()

    init {
        place()
    }

    override fun setVisible(
        first: ULong,
        last: ULong,
    ) {
        reported += first.toInt()..last.toInt()
        place()
        deliver()
    }

    override suspend fun next(): BridgeListView = events.receive().getOrThrow()

    override suspend fun cancel() {
        cancelled = true
        cancelledSignal.complete(Unit)
    }

    /** Place the window around the rows on screen, as core does: three screens from a screen above them. */
    private fun place() {
        val shown = reported.last()
        val screen = shown.last - shown.first + 1
        val start = maxOf(0, shown.first - screen)
        from = if (start == 0) null else list.idAt(start)
        limit = 3 * screen
    }

    fun deliver() {
        events.trySend(list.windowFailure?.let { Result.failure(it) } ?: Result.success(list.value(from, limit)))
    }

    /** Deliver [value] as it is, whatever the list holds. */
    fun deliver(value: BridgeListView) {
        events.trySend(Result.success(value))
    }
}

internal class FakeListPageSubscription(
    private val list: FakeList<*>,
    private val offset: Int,
    private val limit: Int,
) : ListPageSubscription(NoHandle) {
    private val events = Channel<Result<BridgeListRows>>(Channel.UNLIMITED)

    /** The places the page holds. */
    val places: IntRange get() = offset until offset + limit
    var cancelled = false
        private set

    override suspend fun next(): BridgeListRows = events.receive().getOrThrow()

    override suspend fun cancel() {
        cancelled = true
    }

    fun deliver() {
        events.trySend(list.windowFailure?.let { Result.failure(it) } ?: Result.success(list.page(offset, limit)))
    }
}

internal class FakeListCountSubscription(
    private val list: FakeList<*>,
) : ListCountSubscription(NoHandle) {
    private val events = Channel<Result<BridgeListCount>>(Channel.UNLIMITED)
    var cancelled = false
        private set

    /** Completes when the handle cancels the subscription. */
    val cancelledSignal = CompletableDeferred<Unit>()

    override suspend fun next(): BridgeListCount = events.receive().getOrThrow()

    override suspend fun cancel() {
        cancelled = true
        cancelledSignal.complete(Unit)
    }

    fun deliver() {
        events.trySend(list.countFailure?.let { Result.failure(it) } ?: Result.success(list.count()))
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
        extra = { BridgeListCountExtra.AlbumGrid(it > 0, grouped = false) },
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

/** The composer list as the bridge lists it, with no composers. */
internal class FakeComposerList :
    FakeList<BridgeComposerSummary>(
        emptyList(),
        idOf = { it.artistId },
        wrap = { BridgeListRows.Composers(it) },
    )
