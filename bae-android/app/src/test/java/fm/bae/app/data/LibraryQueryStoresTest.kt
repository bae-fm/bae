package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

@OptIn(ExperimentalCoroutinesApi::class)
class LibraryQueryStoresTest {
    /** The search's values reach the store on the main thread, which the test's clock runs. */
    private val main = StandardTestDispatcher()

    @Before
    fun setUp() = Dispatchers.setMain(main)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun switchingSearchQueryClearsPriorValueBeforeTheNewQueryErrors() =
        runTest(main) {
            val failure = queryFailure()
            val handle =
                FakeAppHandle(
                    searchResults = { query ->
                        BridgeFixtures.searchResults(
                            albums = listOf(BridgeFixtures.albumSearchResult(id = "album-$query")),
                        )
                    },
                    initialSearchError = { query -> failure.takeIf { query == "query-b" } },
                )
            val store = SearchQueryStore(Library(handle), backgroundScope)
            store.activate("query-a")
            passDebounce()
            assertTrue(store.state.value.delivered)

            store.activate("query-b")
            assertNull(store.state.value.value)
            assertFalse(store.state.value.delivered)
            assertNull(store.state.value.error)
            passDebounce()

            assertNull(store.state.value.value)
            assertFalse(store.state.value.delivered)
            assertSame(failure, store.state.value.error)
            assertEquals("typing moves one search, never opens another", 1, handle.searchSubscriptions.size)
        }

    @Test
    fun retryOpensTheStandingQueryAgain() =
        runTest(main) {
            val handle =
                FakeAppHandle(
                    searchResults = { BridgeFixtures.searchResults() },
                    initialSearchError = { queryFailure() },
                )
            val store = SearchQueryStore(Library(handle), backgroundScope)
            store.activate("query-a")
            passDebounce()
            assertTrue(store.state.value.error != null)

            store.retry()
            passDebounce()

            assertEquals("the query opens on a new search", 2, handle.searchSubscriptions.size)
        }

    /** Moves the virtual clock to the end of the debounce and runs what it releases. */
    private fun TestScope.passDebounce() {
        advanceTimeBy(SEARCH_DEBOUNCE_MS)
        runCurrent()
    }

    private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.Database, "query failed")
}
