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

    @Test
    fun workDetailFixtureCarriesReleaseRows() {
        val release =
            BridgeFixtures.workReleaseSummary(
                releaseId = "rel-2",
                albumId = "alb-2",
            )
        val detail = BridgeFixtures.workDetail(releases = listOf(release))

        assertFalse(detail.releases.isEmpty())
        assertTrue(detail.releases.any { it.releaseId == "rel-2" })
        assertTrue(detail.releases.any { it.albumId == "alb-2" })
        assertTrue(detail.releases.any { it.albumTitle == "Album Title" })
    }
}
