package fm.bae.app.ui

import uniffi.bae_bridge.BridgeInviteCodeInfo

/**
 * What the invite-code entry shows once the joiner types or scans a code,
 * derived purely from the decode attempt plus this build's provider
 * capabilities. The join action is enabled only for a [Decoded] preview whose
 * provider this device can satisfy ([Decoded.mismatch] is null).
 */
sealed interface JoinInvitePreview {
    /** No code entered yet — nothing to preview. */
    data object Empty : JoinInvitePreview

    /**
     * The code decoded to [info]. When [mismatch] is non-null the library's
     * cloud provider can't be reached from this device, so the join stays
     * blocked and the mismatch notice renders under the preview rows.
     */
    data class Decoded(
        val info: BridgeInviteCodeInfo,
        val mismatch: ProviderMismatch?,
    ) : JoinInvitePreview

    /** The code couldn't be decoded — the joiner sees an invalid-code notice. */
    data object Invalid : JoinInvitePreview
}

/** Why a decoded library's cloud provider can't be joined from this device. */
enum class ProviderMismatch {
    /**
     * This build can sign in to OAuth providers, but the joiner picked a
     * provider that doesn't match this library, so no token is held. Going back
     * to the picker and choosing the library's provider resolves it.
     */
    PROVIDER_NOT_SELECTED,

    /**
     * This build ships no OAuth support (the S3-only edition), so a library that
     * syncs through an OAuth provider can't be joined here at all.
     */
    OAUTH_UNSUPPORTED,
}

/**
 * Map an invite-code decode attempt to the preview the join screen renders.
 *
 * [decode] is null before any code is entered, a success once the code parses,
 * and a failure when it doesn't. [oauthSupported] is whether this edition can
 * connect to any OAuth provider (true for the full edition, false for S3-only).
 * [oauthTokenHeld] is whether the joiner already signed in to the provider it
 * picked, so the eventual join can reuse that token.
 */
fun joinInvitePreview(
    decode: Result<BridgeInviteCodeInfo>?,
    oauthSupported: Boolean,
    oauthTokenHeld: Boolean,
): JoinInvitePreview {
    val result = decode ?: return JoinInvitePreview.Empty
    return result.fold(
        onSuccess = { info ->
            val mismatch =
                when {
                    !info.needsOauth -> null
                    !oauthSupported -> ProviderMismatch.OAUTH_UNSUPPORTED
                    !oauthTokenHeld -> ProviderMismatch.PROVIDER_NOT_SELECTED
                    else -> null
                }
            JoinInvitePreview.Decoded(info, mismatch)
        },
        onFailure = { JoinInvitePreview.Invalid },
    )
}

/** Whether the join action should be enabled for [preview]. */
fun joinEnabled(preview: JoinInvitePreview): Boolean = preview is JoinInvitePreview.Decoded && preview.mismatch == null
