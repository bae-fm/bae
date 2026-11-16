package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryListing

/**
 * The discovered-libraries holder mirrors each listing into its published
 * state, so the Settings switcher shows every local library — a later listing
 * that omits a library drops it (the removal case) — and a listing that failed
 * replaces the last list rather than leaving it to look current.
 */
class DiscoveredLibrariesTest {
    @Test
    fun startsUnlisted() {
        assertEquals(LibraryListing.Unlisted, DiscoveredLibraries().libraries.value)
    }

    @Test
    fun replaceAllPublishesTheScanAndDropsLibrariesAbsentFromALaterScan() {
        val discovered = DiscoveredLibraries()
        val a = BridgeFixtures.library("lib-a")
        val b = BridgeFixtures.library("lib-b")

        discovered.replaceAll(listing(listOf(a, b)))
        assertEquals(LibraryListing.Listed(listing(listOf(a, b))), discovered.libraries.value)

        discovered.replaceAll(listing(listOf(a)))
        assertEquals(LibraryListing.Listed(listing(listOf(a))), discovered.libraries.value)
    }

    @Test
    fun aFailedListingReplacesTheLastListUntilTheNextOne() {
        val discovered = DiscoveredLibraries()
        val a = BridgeFixtures.library("lib-a")
        discovered.replaceAll(listing(listOf(a)))
        val failure = DisplayError("Couldn't read the device's libraries.", "disk I/O error")

        discovered.fail(failure)
        assertEquals(LibraryListing.Failed(failure), discovered.libraries.value)

        discovered.replaceAll(listing(listOf(a)))
        assertEquals(LibraryListing.Listed(listing(listOf(a))), discovered.libraries.value)
    }

    /** A listing of [libraries] that names no neighbors. */
    private fun listing(libraries: List<BridgeLibrary>) =
        BridgeLibraryListing(libraries = libraries, showsSwitcher = libraries.size > 1, activeNeighbors = null)
}
