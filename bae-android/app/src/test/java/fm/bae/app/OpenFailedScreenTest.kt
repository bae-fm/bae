package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test

/** Where a failed open of a library lands: one screen for each way on core names. */
class OpenFailedScreenTest {
    @Test
    fun anUnfinishedRemovalLandsOnFinishingTheRemoval() {
        assertEquals(
            AppScreen.DeletionUnfinished("library", error = null),
            AppScreen.openFailed(
                "library",
                OpenFailureOffer.FINISH_REMOVAL,
                DisplayError("Removing did not finish."),
                open = null,
            ),
        )
    }

    private val damaged = DisplayError("Something went wrong reading your library.", "damaged", offersRetry = false)

    @Test
    fun aFailureAResetLoadsAgainOffersTheResetInPlaceOfRetry() {
        assertEquals(
            AppScreen.ResetOffered("library", ScreenFailure(FailedStep.LIBRARY_OPEN, damaged)),
            AppScreen.openFailed("library", OpenFailureOffer.RESET_FROM_CLOUD, damaged, open = null),
        )
    }

    /** Core fails a damaged copy of a library with no storage plainly: there is nothing to reset from. */
    @Test
    fun aDamagedLibraryCoreFailsPlainlyOffersNoReset() {
        assertEquals(
            AppScreen.Failed(ScreenFailure(FailedStep.LIBRARY_OPEN, damaged)),
            AppScreen.openFailed("library", OpenFailureOffer.NOTHING, damaged, open = null),
        )
    }

    @Test
    fun anyOtherFailureIsShownAsItIs() {
        assertEquals(
            AppScreen.Failed(ScreenFailure(FailedStep.LIBRARY_OPEN, DisplayError("Offline.", "offline"))),
            AppScreen.openFailed("library", OpenFailureOffer.NOTHING, DisplayError("Offline.", "offline"), open = null),
        )
    }
}
