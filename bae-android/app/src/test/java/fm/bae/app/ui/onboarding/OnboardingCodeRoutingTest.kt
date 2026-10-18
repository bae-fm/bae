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

    @Test
    fun aRestoreCodeOpensTheRestoreFormWithTheKeyThatCameBesideIt() {
        val key = BridgeS3Credentials(accessKeyId = "key-id", secretAccessKey = "secret")

        val step = codeReadStep("restore-code", BridgeCodeInfo.Restore(metadata), key)

        assertEquals(OnboardingStep.Restore("restore-code", metadata, key), step)
    }

    @Test
    fun anInviteOpensTheJoinFormForTheLibraryItNames() {
        val step = codeReadStep("invite-code", BridgeCodeInfo.Invite(metadata, "Household"), null)

        assertEquals(OnboardingStep.Invite("invite-code", metadata, "Household"), step)
    }
}
