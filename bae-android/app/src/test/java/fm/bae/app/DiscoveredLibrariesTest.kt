package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeLibrary

/**
 * The discovered-libraries holder mirrors each listing into its published list,
 * so the Settings switcher shows every local library — a later listing that
 * omits a library drops it (the removal case).
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
}
