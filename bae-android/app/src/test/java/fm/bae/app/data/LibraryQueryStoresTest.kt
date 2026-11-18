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

    /** The last text's results stay while the new text's query runs, and the new text's failure replaces them. */
    @Test
    fun switchingSearchQueryKeepsThePriorResultsUntilTheNewQueryAnswers() =
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
            assertEquals(
                "query-a",
                store.state.value.answer
                    ?.query,
            )

            store.activate("query-b")
            assertEquals(
                "query-a",
                store.state.value.answer
                    ?.query,
            )
            passDebounce()

            assertEquals(LiveQueryState.Failed(failure), store.state.value)
            assertEquals("typing moves one search, never opens another", 1, handle.searchSubscriptions.size)
        }

    /** A new text's results replace the last text's once its query answers. */
    @Test
    fun aNewTextsResultsReplaceTheLastOnes() =
        runTest(main) {
            val handle =
                FakeAppHandle(
                    searchResults = { query ->
                        BridgeFixtures.searchResults(
                            albums = listOf(BridgeFixtures.albumSearchResult(id = "album-$query")),
                        )
                    },
                )
            val store = SearchQueryStore(Library(handle), backgroundScope)
            store.activate("query-a")
            passDebounce()

            store.activate("query-b")
            passDebounce()

            assertEquals(
                listOf("album-query-b"),
                store.state.value.answer
                    ?.results
                    ?.albums
                    ?.map { it.id },
            )
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
            assertTrue(store.state.value is LiveQueryState.Failed)

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
