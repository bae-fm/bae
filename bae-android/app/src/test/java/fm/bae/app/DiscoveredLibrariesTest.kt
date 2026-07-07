package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeLibrary

/**
 * The discovered-libraries holder mirrors each scan into its published list and
 * appends a freshly-linked library exactly once, so the Settings switcher shows
 * every local library — a later scan that omits a library drops it (the forget
 * case), and re-noting a library the scan already found does not duplicate it.
 */
class DiscoveredLibrariesTest {
    @Test
    fun startsEmpty() {
        assertEquals(emptyList<BridgeLibrary>(), DiscoveredLibraries().libraries.value)
    }

    @Test
    fun replaceAllPublishesTheScanAndDropsLibrariesAbsentFromALaterScan() {
        val discovered = DiscoveredLibraries()
        val a = BridgeFixtures.library("lib-a")
        val b = BridgeFixtures.library("lib-b")

        discovered.replaceAll(listOf(a, b))
        assertEquals(listOf(a, b), discovered.libraries.value)

        discovered.replaceAll(listOf(a))
        assertEquals(listOf(a), discovered.libraries.value)
    }

    @Test
    fun noteLinkedAppendsAnUnknownLibrary() {
        val discovered = DiscoveredLibraries()
        val a = BridgeFixtures.library("lib-a")
        val b = BridgeFixtures.library("lib-b")
        discovered.replaceAll(listOf(a))

        discovered.noteLinked(b)

        assertEquals(listOf(a, b), discovered.libraries.value)
    }

    @Test
    fun noteLinkedWithAKnownIdLeavesTheListUnchanged() {
        val discovered = DiscoveredLibraries()
        val a = BridgeFixtures.library("lib-a")
        discovered.replaceAll(listOf(a))

        discovered.noteLinked(BridgeFixtures.library("lib-a", name = "Renamed"))

        assertEquals(listOf(a), discovered.libraries.value)
    }
}
