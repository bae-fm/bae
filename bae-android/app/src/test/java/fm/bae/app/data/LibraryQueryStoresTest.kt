package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

class LibraryQueryStoresTest {
    @Test
    fun switchingSearchQueryClearsPriorValueBeforeTheNewQueryErrors() =
        runBlocking {
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
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val store = SearchQueryStore(Library(handle), scope)
            store.activate("query-a")
            delay(350)
            assertTrue(store.state.value.delivered)

            store.activate("query-b")
            assertNull(store.state.value.value)
            assertFalse(store.state.value.delivered)
            assertNull(store.state.value.error)
            delay(350)

            assertNull(store.state.value.value)
            assertFalse(store.state.value.delivered)
            assertSame(failure, store.state.value.error)
            scope.cancel()
        }

    private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.DATABASE, "query failed")
}
