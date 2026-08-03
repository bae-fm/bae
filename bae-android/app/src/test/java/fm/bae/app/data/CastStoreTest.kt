package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The cast button and the picker's active row read the casting device off the
 * status core last announced, so the store must surface the latest one — a
 * receiver-side end arrives the same way a user stop does.
 */
class CastStoreTest {
    @Test
    fun statusCarriesTheCastingDevice() {
        val store = CastStore()
        assertNull(castingDeviceName(store.status.value))

        store.applyStatus("Living Room Speaker")
        assertEquals("Living Room Speaker", castingDeviceName(store.status.value))

        store.applyStatus(null)
        assertNull(castingDeviceName(store.status.value))
    }

    @Test
    fun devicesSurfaceTheLatestList() {
        val store = CastStore()
        assertTrue(store.devices.value.isEmpty())

        store.setDevices(
            listOf(
                BridgeFixtures.castDevice(id = "cast-1", name = "Kitchen"),
                BridgeFixtures.castDevice(id = "cast-2", name = "Study"),
            ),
        )
        assertEquals(listOf("Kitchen", "Study"), store.devices.value.map { it.name })

        store.setDevices(emptyList())
        assertTrue(store.devices.value.isEmpty())
    }
}
