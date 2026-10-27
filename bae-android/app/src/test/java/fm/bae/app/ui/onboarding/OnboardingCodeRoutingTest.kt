package fm.bae.app.ui.onboarding

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeInfo
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeInviteDeliveryKind
import uniffi.bae_bridge.BridgeInviteInfo
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
            carriesS3Key = false,
        )

    private val restoreOver =
        RestoreOver(libraryId = "library-1", title = "Restore this library", message = "Warning", onBack = {})

    @Test
    fun aRestoreCodeOpensTheRestoreForm() {
        val step = codeReadStep("restore-code", BridgeCodeInfo.Restore(metadata), CodeEntry.NewDevice)

        assertEquals(OnboardingStep.Restore("restore-code", metadata, CodeEntry.NewDevice), step)
    }

    private val invite =
        BridgeInviteInfo(
            metadata = metadata,
            libraryName = "Household",
            invitee = "Dan",
            delivery = BridgeInviteDeliveryKind.IN_PERSON,
        )

    @Test
    fun anInviteJoinsTheLibraryItNamesFromItsCode() {
        val step = codeReadStep("invite-code", BridgeCodeInfo.Invite(invite), CodeEntry.NewDevice)

        assertEquals(OnboardingStep.Invite(JoinStart.Code("invite-code"), invite, CodeEntry.NewDevice), step)
    }

    @Test
    fun anInviteEnteredToRestoreOverThisDevicesCopyGoesToTheRestore() {
        val step = codeReadStep("invite-code", BridgeCodeInfo.Invite(invite), restoreOver)

        assertEquals(OnboardingStep.Restore("invite-code", metadata, restoreOver), step)
    }
}
