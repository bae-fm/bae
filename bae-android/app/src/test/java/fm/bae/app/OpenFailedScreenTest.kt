package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeOpenException

/** Where a failed open of a library lands: one screen for each way on core names. */
class OpenFailedScreenTest {
    @Test
    fun anUnfinishedRemovalLandsOnFinishingTheRemoval() {
        val failure =
            BridgeOpenException.DeletionUnfinished(
                BridgeException.Diagnostic(BridgeErrorCategory.DeletionUnfinished, "deletion unfinished"),
            )

        assertEquals(
            AppScreen.DeletionUnfinished("library", error = null),
            AppScreen.openFailed("library", failure, DisplayError("Removing did not finish."), open = null),
        )
    }

    private val damaged = DisplayError("Something went wrong reading your library.", "damaged", offersRetry = false)

    @Test
    fun aFailureAResetLoadsAgainOffersTheResetInPlaceOfRetry() {
        val failure =
            BridgeOpenException.ResetFromCloud(BridgeException.Diagnostic(BridgeErrorCategory.DatabaseDamaged, "damaged"))

        assertEquals(
            AppScreen.ResetOffered("library", ScreenFailure(FailedStep.LIBRARY_OPEN, damaged)),
            AppScreen.openFailed("library", failure, damaged, open = null),
        )
    }

    /** Core fails a damaged copy of a library with no storage plainly: there is nothing to reset from. */
    @Test
    fun aDamagedLibraryCoreFailsPlainlyOffersNoReset() {
        val failure = BridgeOpenException.Failed(BridgeException.Diagnostic(BridgeErrorCategory.DatabaseDamaged, "damaged"))

        assertEquals(
            AppScreen.Failed(ScreenFailure(FailedStep.LIBRARY_OPEN, damaged)),
            AppScreen.openFailed("library", failure, damaged, open = null),
        )
    }

    @Test
    fun anyOtherFailureIsShownAsItIs() {
        val failure = BridgeOpenException.Failed(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline"))

        assertEquals(
            AppScreen.Failed(ScreenFailure(FailedStep.LIBRARY_OPEN, DisplayError("Offline.", "offline"))),
            AppScreen.openFailed("library", failure, DisplayError("Offline.", "offline"), open = null),
        )
    }
}
