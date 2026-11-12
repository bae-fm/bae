package fm.bae.app.ui.settings

import fm.bae.app.BridgeFixtures
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.colors
import fm.bae.app.ui.components.color
import fm.bae.app.ui.components.storageUsageSegments
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeStorageKind

/** The usage bar's segments follow core's parts: their order, their share of the total, and a colour for each kind. */
class StorageUsageSegmentsTest {
    private fun segments(beforeReset: ULong? = null) =
        storageUsageSegments(BridgeFixtures.deviceStorage(beforeReset), { it.name }, { "$it bytes" })

    @Test
    fun theSegmentsAreCoresPartsInCoresOrder() {
        assertEquals(
            listOf(BridgeStorageKind.PINNED, BridgeStorageKind.CACHE, BridgeStorageKind.LIBRARY),
            segments().map { it.kind },
        )
        assertEquals(
            listOf(
                BridgeStorageKind.PINNED,
                BridgeStorageKind.CACHE,
                BridgeStorageKind.LIBRARY,
                BridgeStorageKind.BEFORE_RESET,
            ),
            segments(beforeReset = 540_000_000uL).map { it.kind },
        )
    }

    @Test
    fun aKindCoreLeavesOutIsNotDrawn() {
        val full = BridgeFixtures.deviceStorage()
        val storage = full.copy(parts = full.parts.filter { it.kind != BridgeStorageKind.CACHE })

        val kinds = storageUsageSegments(storage, { it.name }, { it.toString() }).map { it.kind }

        assertEquals(listOf(BridgeStorageKind.PINNED, BridgeStorageKind.LIBRARY), kinds)
    }

    @Test
    fun eachShareIsItsBytesOverTheTotal() {
        val shares = segments().map { it.share }

        assertEquals(2_400_000_000.0 / 3_860_000_000.0, shares[0].toDouble(), 1e-6)
        assertEquals(1_200_000_000.0 / 3_860_000_000.0, shares[1].toDouble(), 1e-6)
        assertEquals(260_000_000.0 / 3_860_000_000.0, shares[2].toDouble(), 1e-6)
    }

    @Test
    fun eachSegmentCarriesItsLabelAndSize() {
        assertEquals(listOf("PINNED", "CACHE", "LIBRARY"), segments().map { it.label })
        assertEquals(listOf("2400000000 bytes", "1200000000 bytes", "260000000 bytes"), segments().map { it.size })
    }

    @Test
    fun eachKindKeepsItsAccentsSelectionColour() {
        assertEquals(AccentChoice.BLUE.colors.selection, BridgeStorageKind.PINNED.color)
        assertEquals(AccentChoice.GOLD.colors.selection, BridgeStorageKind.CACHE.color)
        assertEquals(AccentChoice.ORCHID.colors.selection, BridgeStorageKind.LIBRARY.color)
        assertEquals(AccentChoice.MINT.colors.selection, BridgeStorageKind.BEFORE_RESET.color)
        assertEquals(
            4,
            BridgeStorageKind.entries
                .map { it.color }
                .toSet()
                .size,
        )
        assertNotEquals(BridgeStorageKind.PINNED.color, BridgeStorageKind.CACHE.color)
    }
}
