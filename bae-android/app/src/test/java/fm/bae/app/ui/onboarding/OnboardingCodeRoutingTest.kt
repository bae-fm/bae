package fm.bae.app.ui.onboarding

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeInfo
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeStorageLocation

class OnboardingCodeRoutingTest {
    private val metadata =
        BridgeCodeMetadata(
            libraryId = "library-1",
            memberId = "member-1",
            provider = BridgeCloudProvider.ONE_DRIVE,
            location =
                BridgeStorageLocation.OneDrive(
                    driveId = "drive-1",
                    folderId = "folder-1",
                    accountLabel = "person@example.com",
                ),
            oauthAccess = null,
        )

    private val restoreOver =
        RestoreOver(libraryId = "library-1", title = "Restore this library", message = "Warning", onBack = {})

    @Test
    fun aRestoreCodeOpensTheRestoreFormWithTheKeyThatCameBesideIt() {
        val key = BridgeS3Credentials(accessKeyId = "key-id", secretAccessKey = "secret")

        val step = codeReadStep("restore-code", BridgeCodeInfo.Restore(metadata), key, CodeEntry.AddDevice)

        assertEquals(OnboardingStep.Restore("restore-code", metadata, key, CodeEntry.AddDevice), step)
    }

    @Test
    fun anInviteOpensTheJoinFormForTheLibraryItNames() {
        val step = codeReadStep("invite-code", BridgeCodeInfo.Invite(metadata, "Household"), null, CodeEntry.JoinLibrary)

        assertEquals(OnboardingStep.Invite("invite-code", metadata, "Household", CodeEntry.JoinLibrary), step)
    }

    @Test
    fun anInviteEnteredAfterChoosingAddDeviceStillJoins() {
        val step = codeReadStep("invite-code", BridgeCodeInfo.Invite(metadata, "Household"), null, CodeEntry.AddDevice)

        assertEquals(OnboardingStep.Invite("invite-code", metadata, "Household", CodeEntry.AddDevice), step)
    }

    @Test
    fun aRestoreCodeEnteredAfterChoosingJoinLibraryStillRestores() {
        val step = codeReadStep("restore-code", BridgeCodeInfo.Restore(metadata), null, CodeEntry.JoinLibrary)

        assertEquals(OnboardingStep.Restore("restore-code", metadata, null, CodeEntry.JoinLibrary), step)
    }

    @Test
    fun anInviteEnteredToRestoreOverThisDevicesCopyGoesToTheRestore() {
        val step = codeReadStep("invite-code", BridgeCodeInfo.Invite(metadata, "Household"), null, restoreOver)

        assertEquals(OnboardingStep.Restore("invite-code", metadata, null, restoreOver), step)
    }
}
