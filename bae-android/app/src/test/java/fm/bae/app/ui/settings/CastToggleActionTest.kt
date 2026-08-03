package fm.bae.app.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Turning casting off ends a session in flight, so the settings toggle asks
 * first in exactly that case. Every other flip writes straight through — asking
 * when nothing is casting would be a dialog about nothing.
 */
class CastToggleActionTest {
    @Test
    fun turningCastingOnNeverAsks() {
        assertEquals(
            CastToggleAction.Apply(true),
            castToggleAction(enabled = true, castingDeviceName = null),
        )
        assertEquals(
            CastToggleAction.Apply(true),
            castToggleAction(enabled = true, castingDeviceName = "Living Room Speaker"),
        )
    }

    @Test
    fun turningCastingOffWithNothingCastingAppliesDirectly() {
        assertEquals(
            CastToggleAction.Apply(false),
            castToggleAction(enabled = false, castingDeviceName = null),
        )
    }

    @Test
    fun turningCastingOffMidSessionAsksNamingTheDevice() {
        assertEquals(
            CastToggleAction.ConfirmDisconnect("Living Room Speaker"),
            castToggleAction(enabled = false, castingDeviceName = "Living Room Speaker"),
        )
    }
}
