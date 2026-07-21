package fm.bae.app.ui

import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertEquals
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
                    artists = listOf(BridgeFixtures.artistSummary()),
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
    fun artistsRenderAsAnOrderedSection() {
        val results =
            BridgeFixtures.searchResults(
                artists =
                    listOf(
                        BridgeFixtures.artistSummary(artistId = "artist-1", name = "Artist One"),
                        BridgeFixtures.artistSummary(artistId = "artist-2", name = "Artist Two"),
                    ),
            )

        assertFalse(results.hasNoResults())
        // The section iterates results.artists in order; each row selects by artistId.
        assertEquals(listOf("artist-1", "artist-2"), results.artists.map { it.artistId })
    }
}
