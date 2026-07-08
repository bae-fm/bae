package fm.bae.app.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeInviteCodeInfo

/**
 * The invite-preview mapping — how a decode attempt plus this build's provider
 * capabilities become the preview the join screen shows and the join gate. The
 * native decode is stubbed with [Result] values so the mapping is exercised
 * without the Rust runtime.
 */
class JoinInvitePreviewTest {
    private fun invite(
        needsOauth: Boolean,
        provider: BridgeCloudProvider,
    ): BridgeInviteCodeInfo =
        BridgeInviteCodeInfo(
            libraryId = "lib-1",
            libraryName = "bae Library",
            ownerPubkey = "pubkey-1",
            ownerFingerprint = "abcd1234",
            cloudProvider = provider,
            needsOauth = needsOauth,
        )

    @Test
    fun noCodeEnteredIsEmptyAndBlocked() {
        val preview = joinInvitePreview(decode = null, oauthSupported = true, oauthTokenHeld = false)
        assertEquals(JoinInvitePreview.Empty, preview)
        assertFalse(joinEnabled(preview))
    }

    @Test
    fun undecodableCodeIsInvalidAndBlocked() {
        val decode = Result.failure<BridgeInviteCodeInfo>(IllegalArgumentException("bad code"))
        val preview = joinInvitePreview(decode, oauthSupported = true, oauthTokenHeld = false)
        assertEquals(JoinInvitePreview.Invalid, preview)
        assertFalse(joinEnabled(preview))
    }

    @Test
    fun s3LibraryNeedsNoProviderAndJoins() {
        val decode = Result.success(invite(needsOauth = false, provider = BridgeCloudProvider.S3))
        val preview = joinInvitePreview(decode, oauthSupported = true, oauthTokenHeld = false)
        assertEquals(JoinInvitePreview.Decoded(decode.getOrThrow(), mismatch = null), preview)
        assertTrue(joinEnabled(preview))
    }

    @Test
    fun oauthLibraryWithTokenJoins() {
        val decode = Result.success(invite(needsOauth = true, provider = BridgeCloudProvider.GOOGLE_DRIVE))
        val preview = joinInvitePreview(decode, oauthSupported = true, oauthTokenHeld = true)
        assertEquals(JoinInvitePreview.Decoded(decode.getOrThrow(), mismatch = null), preview)
        assertTrue(joinEnabled(preview))
    }

    @Test
    fun oauthLibraryWithoutTokenBlocksOnProviderChoice() {
        val decode = Result.success(invite(needsOauth = true, provider = BridgeCloudProvider.DROPBOX))
        val preview = joinInvitePreview(decode, oauthSupported = true, oauthTokenHeld = false)
        assertEquals(
            JoinInvitePreview.Decoded(decode.getOrThrow(), ProviderMismatch.PROVIDER_NOT_SELECTED),
            preview,
        )
        assertFalse(joinEnabled(preview))
    }

    @Test
    fun oauthLibraryUnsupportedByEditionBlocks() {
        val decode = Result.success(invite(needsOauth = true, provider = BridgeCloudProvider.ONE_DRIVE))
        val preview = joinInvitePreview(decode, oauthSupported = false, oauthTokenHeld = false)
        assertEquals(
            JoinInvitePreview.Decoded(decode.getOrThrow(), ProviderMismatch.OAUTH_UNSUPPORTED),
            preview,
        )
        assertFalse(joinEnabled(preview))
    }

    @Test
    fun unsupportedEditionWinsOverAnyHeldToken() {
        // An edition with no OAuth support can never satisfy an OAuth library,
        // even if some token were somehow held — the unsupported state stands.
        val decode = Result.success(invite(needsOauth = true, provider = BridgeCloudProvider.GOOGLE_DRIVE))
        val preview = joinInvitePreview(decode, oauthSupported = false, oauthTokenHeld = true)
        assertEquals(
            JoinInvitePreview.Decoded(decode.getOrThrow(), ProviderMismatch.OAUTH_UNSUPPORTED),
            preview,
        )
        assertFalse(joinEnabled(preview))
    }
}
