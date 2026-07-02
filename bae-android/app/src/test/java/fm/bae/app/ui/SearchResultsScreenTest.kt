package fm.bae.app.ui

import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SearchResultsScreenTest {
    @Test
    fun emptyStateConsidersEverySearchSection() {
        assertTrue(BridgeFixtures.searchResults().hasNoResults())
        assertFalse(
            BridgeFixtures
                .searchResults(
                    albums = listOf(BridgeFixtures.albumSearchResult()),
                ).hasNoResults(),
        )
        assertFalse(
            BridgeFixtures
                .searchResults(
                    tracks = listOf(BridgeFixtures.trackSearchResult()),
                ).hasNoResults(),
        )
        assertFalse(
            BridgeFixtures
                .searchResults(
                    composers = listOf(BridgeFixtures.composerSummary()),
                ).hasNoResults(),
        )
        assertFalse(
            BridgeFixtures
                .searchResults(
                    works = listOf(BridgeFixtures.workSummary()),
                ).hasNoResults(),
        )
    }
}
