package fm.bae.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AlbumPageCallback
import uniffi.bae_bridge.ArtistPageCallback
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumPage
import uniffi.bae_bridge.BridgeArtistPage
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerPage
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.ComposerPageCallback

internal const val BROWSER_PAGE_SIZE = 60
private const val MAXIMUM_BROWSER_PAGE_SUBSCRIPTIONS = 3
private val logger = BaeLogger("bae.BrowserPageStores")

internal fun interface PageSubscription {
    fun cancel()
}

private data class ActivePageSubscription(
    val identity: Long,
    val subscription: PageSubscription,
)

internal class PageError(
    val message: String,
    val onRetry: () -> Unit,
)

internal abstract class WindowedBrowserPageStore<Parameter, Row>(
    private val appContext: Context,
    protected val scope: CoroutineScope,
) {
    val rows = mutableStateMapOf<Int, Row>()
    var totalCount by mutableStateOf(0)
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<PageError?>(null)
        private set

    private var parameter: Parameter? = null
    private var active = false
    private var generation = 0
    private var nextSubscriptionIdentity = 0L
    private val subscriptions = mutableMapOf<Int, ActivePageSubscription>()

    fun activate(parameter: Parameter) {
        if (active && this.parameter == parameter) return
        cancelSubscriptions()
        rows.clear()
        totalCount = 0
        loading = true
        error = null
        this.parameter = parameter
        active = true
        generation++
        subscribe(offset = 0, generation = generation)
    }

    fun deactivate() {
        active = false
        cancelSubscriptions()
        rows.clear()
        totalCount = 0
    }

    fun reportVisibleRange(
        first: Int,
        last: Int,
    ) {
        if (!active || first > last) return
        val end = if (totalCount == 0) BROWSER_PAGE_SIZE else totalCount
        val firstPage = first.coerceIn(0, end) / BROWSER_PAGE_SIZE * BROWSER_PAGE_SIZE
        val lastPage = last.coerceIn(0, end) / BROWSER_PAGE_SIZE * BROWSER_PAGE_SIZE
        val wanted =
            generateSequence(firstPage) { offset ->
                (offset + BROWSER_PAGE_SIZE).takeIf { it <= lastPage }
            }.take(MAXIMUM_BROWSER_PAGE_SUBSCRIPTIONS)
                .toSet()

        subscriptions.keys.filter { it !in wanted }.forEach { offset ->
            subscriptions.remove(offset)?.subscription?.cancel()
            removePage(offset)
        }
        wanted.forEach { offset -> subscribe(offset, generation) }
    }

    fun retry() {
        val current = checkNotNull(parameter) { "browser page retry without parameters" }
        active = false
        activate(current)
    }

    protected fun deliver(
        offset: Int,
        deliveredGeneration: Int,
        deliveredIdentity: Long,
        deliveredRows: List<Row>,
        total: Int,
    ) {
        scope.launch(Dispatchers.Main.immediate) {
            if (
                !active ||
                deliveredGeneration != generation ||
                subscriptions[offset]?.identity != deliveredIdentity
            ) {
                return@launch
            }
            removePage(offset)
            deliveredRows.forEachIndexed { index, row -> rows[offset + index] = row }
            rows.keys.filter { it >= total }.forEach(rows::remove)
            totalCount = total
            loading = false
            error = null
        }
    }

    protected fun fail(
        offset: Int,
        deliveredGeneration: Int,
        deliveredIdentity: Long,
        value: BridgeException,
    ) {
        scope.launch(Dispatchers.Main.immediate) {
            if (
                !active ||
                deliveredGeneration != generation ||
                subscriptions[offset]?.identity != deliveredIdentity
            ) {
                return@launch
            }
            logger.error("browser page subscription failed at offset $offset", value)
            loading = false
            error =
                PageError(
                    value.message
                        ?: appContext.getString(
                            if (offset == 0) R.string.library_load_failed else R.string.library_load_more_failed,
                        ),
                    ::retry,
                )
        }
    }

    private fun subscribe(
        offset: Int,
        generation: Int,
    ) {
        val current = parameter ?: return
        if (subscriptions.containsKey(offset)) return
        val identity = ++nextSubscriptionIdentity
        subscriptions[offset] = ActivePageSubscription(identity, PageSubscription {})
        val subscription = subscribe(current, offset, generation, identity)
        if (subscriptions[offset]?.identity == identity) {
            subscriptions[offset] = ActivePageSubscription(identity, subscription)
        } else {
            subscription.cancel()
        }
    }

    protected abstract fun subscribe(
        parameter: Parameter,
        offset: Int,
        generation: Int,
        identity: Long,
    ): PageSubscription

    private fun removePage(offset: Int) {
        repeat(BROWSER_PAGE_SIZE) { rows.remove(offset + it) }
    }

    private fun cancelSubscriptions() {
        subscriptions.values.forEach { it.subscription.cancel() }
        subscriptions.clear()
    }
}

internal class AlbumPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeSortCriterion, BridgeAlbum>(appContext, scope) {
    override fun subscribe(
        parameter: BridgeSortCriterion,
        offset: Int,
        generation: Int,
        identity: Long,
    ): PageSubscription {
        val subscription =
            library.subscribeAlbumPage(
                listOf(parameter),
                offset.toULong(),
                BROWSER_PAGE_SIZE.toULong(),
                object : AlbumPageCallback {
                    override fun onValue(value: BridgeAlbumPage) =
                        deliver(offset, generation, identity, value.rows, value.totalCount.toInt())

                    override fun onError(errorValue: BridgeException) =
                        fail(offset, generation, identity, errorValue)
                },
            )
        return PageSubscription(subscription::cancel)
    }
}

internal class ArtistPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeArtistSortCriterion, BridgeArtistSummary>(appContext, scope) {
    override fun subscribe(
        parameter: BridgeArtistSortCriterion,
        offset: Int,
        generation: Int,
        identity: Long,
    ): PageSubscription {
        val subscription =
            library.subscribeArtistPage(
                parameter,
                offset.toULong(),
                BROWSER_PAGE_SIZE.toULong(),
                object : ArtistPageCallback {
                    override fun onValue(value: BridgeArtistPage) =
                        deliver(offset, generation, identity, value.rows, value.totalCount.toInt())

                    override fun onError(errorValue: BridgeException) =
                        fail(offset, generation, identity, errorValue)
                },
            )
        return PageSubscription(subscription::cancel)
    }
}

internal class ComposerPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeComposerSortCriterion, BridgeComposerSummary>(appContext, scope) {
    override fun subscribe(
        parameter: BridgeComposerSortCriterion,
        offset: Int,
        generation: Int,
        identity: Long,
    ): PageSubscription {
        val subscription =
            library.subscribeComposerPage(
                parameter,
                offset.toULong(),
                BROWSER_PAGE_SIZE.toULong(),
                object : ComposerPageCallback {
                    override fun onValue(value: BridgeComposerPage) =
                        deliver(offset, generation, identity, value.rows, value.totalCount.toInt())

                    override fun onError(errorValue: BridgeException) =
                        fail(offset, generation, identity, errorValue)
                },
            )
        return PageSubscription(subscription::cancel)
    }
}

internal class BrowserPageStores(
    library: Library,
    appContext: Context,
    scope: CoroutineScope,
) {
    val albums = AlbumPageStore(library, appContext, scope)
    val artists = ArtistPageStore(library, appContext, scope)
    val composers = ComposerPageStore(library, appContext, scope)

    fun cancel() {
        albums.deactivate()
        artists.deactivate()
        composers.deactivate()
    }
}
