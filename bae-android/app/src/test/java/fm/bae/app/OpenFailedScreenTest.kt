package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeReplacedBy

/** Where a failed open of a library lands. */
class OpenFailedScreenTest {
    @Test
    fun anUnfinishedRestoreLandsOnFinishingTheRestore() {
        val failure =
            BridgeException.Diagnostic(
                BridgeErrorCategory.ReplacementUnfinished(BridgeReplacedBy.RESTORE),
                "restore_device",
            )

        assertEquals(
            AppScreen.RestoreUnfinished("library"),
            AppScreen.openFailed("library", failure, "Restoring did not finish."),
        )
    }

    @Test
    fun anUnfinishedResetLandsOnFinishingTheReset() {
        val failure =
            BridgeException.Diagnostic(
                BridgeErrorCategory.ReplacementUnfinished(BridgeReplacedBy.RESET),
                "reset_device",
            )

        assertEquals(
            AppScreen.ResetUnfinished("library", error = null),
            AppScreen.openFailed("library", failure, "Resetting did not finish."),
        )
    }

    @Test
    fun aDamagedCopyOffersAReset() {
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.DeviceResetRequired, "damaged database")

        assertEquals(
            AppScreen.ResetOffered("library", "This device must be reset."),
            AppScreen.openFailed("library", failure, "This device must be reset."),
        )
    }

    @Test
    fun anyOtherFailureIsShownAsItIs() {
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

        assertEquals(
            AppScreen.Failed("Offline."),
            AppScreen.openFailed("library", failure, "Offline."),
        )
    }
}
