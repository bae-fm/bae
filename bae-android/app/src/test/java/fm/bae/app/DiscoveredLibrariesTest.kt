package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test

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

        discovered.replaceAll(listOf(a, b))
        assertEquals(LibraryListing.Listed(listOf(a, b)), discovered.libraries.value)

        discovered.replaceAll(listOf(a))
        assertEquals(LibraryListing.Listed(listOf(a)), discovered.libraries.value)
    }

    @Test
    fun aFailedListingReplacesTheLastListUntilTheNextOne() {
        val discovered = DiscoveredLibraries()
        val a = BridgeFixtures.library("lib-a")
        discovered.replaceAll(listOf(a))
        val failure = DisplayError("Couldn't read the device's libraries.", "disk I/O error")

        discovered.fail(failure)
        assertEquals(LibraryListing.Failed(failure), discovered.libraries.value)

        discovered.replaceAll(listOf(a))
        assertEquals(LibraryListing.Listed(listOf(a)), discovered.libraries.value)
    }
}
