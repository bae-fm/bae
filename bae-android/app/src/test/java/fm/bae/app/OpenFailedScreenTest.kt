package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeStorageLocation

/** Where a failed open of a library lands. */
class OpenFailedScreenTest {
    @Test
    fun anUnfinishedRemovalLandsOnFinishingTheRemoval() {
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.DeletionUnfinished, "deletion unfinished")

        assertEquals(
            AppScreen.DeletionUnfinished("library", error = null),
            AppScreen.openFailed("library", failure, DisplayError("Removing did not finish."), offersReset = false, open = null),
        )
    }

    @Test
    fun aDamagedCopyOffersAReset() {
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.DeviceResetRequired, "damaged database")

        assertEquals(
            AppScreen.ResetOffered(
                "library",
                ScreenFailure(FailedStep.LIBRARY_OPEN, DisplayError("This device must be reset.", "damaged database")),
            ),
            AppScreen.openFailed(
                "library",
                failure,
                DisplayError("This device must be reset.", "damaged database"),
                offersReset = false,
                open = null,
            ),
        )
    }

    @Test
    fun anyOtherFailureIsShownAsItIs() {
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

        assertEquals(
            AppScreen.Failed(ScreenFailure(FailedStep.LIBRARY_OPEN, DisplayError("Offline.", "offline"))),
            AppScreen.openFailed("library", failure, DisplayError("Offline.", "offline"), offersReset = false, open = null),
        )
    }

    private val damaged =
        DisplayError("Something went wrong with the database.", "damaged", offersRetry = false, resetFromCloudReplaces = true)

    private fun library(storage: BridgeStorageLocation?) =
        BridgeLibrary("library", "Library", "/libraries/library", BridgeLibraryState.Openable(storage), isActive = false)

    private val dropbox =
        BridgeStorageLocation.Dropbox(
            BridgeDropboxLocation.AppFolder,
            accountLabel = "ana@example.com",
            folderLabel = null,
        )

    @Test
    fun aDamagedLibraryOffersAResetInPlaceOfRetry() {
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.DatabaseDamaged, "damaged")

        assertEquals(
            AppScreen.ResetOffered("library", ScreenFailure(FailedStep.LIBRARY_OPEN, damaged)),
            AppScreen.openFailed("library", failure, damaged, library(dropbox).offersResetFromCloud(damaged), open = null),
        )
    }

    @Test
    fun aDamagedLibraryWithNoStorageOffersNoReset() {
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.DatabaseDamaged, "damaged")

        assertFalse(library(storage = null).offersResetFromCloud(damaged))
        assertEquals(
            AppScreen.Failed(ScreenFailure(FailedStep.LIBRARY_OPEN, damaged)),
            AppScreen.openFailed("library", failure, damaged, library(storage = null).offersResetFromCloud(damaged), open = null),
        )
    }

    @Test
    fun onlyAFailureInTheLibrarysOwnDataOffersAReset() {
        assertTrue(library(dropbox).offersResetFromCloud(damaged))
        assertFalse(library(dropbox).offersResetFromCloud(DisplayError("Offline.", "offline")))
        assertFalse(null.offersResetFromCloud(damaged))
    }
}
