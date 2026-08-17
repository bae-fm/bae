package fm.bae.app.ui.onboarding

import fm.bae.app.OAuthLinker
import uniffi.bae_bridge.isDevicePairingCode

/** Which onboarding code the QR scanner is currently capturing. */
internal enum class ScanTarget {
    ANY_SETUP_CODE,
    PAIRING_CODE,
}

internal class OnboardingCodeRouter(
    private val launcher: LinkLauncher,
    private val joinLauncher: JoinLauncher,
    private val oauthLinking: OAuthLinker?,
    private val oauthLinkingError: String?,
    private val setShowJoin: (Boolean) -> Unit,
) {
    fun setScanError(
        target: ScanTarget,
        message: String?,
    ) {
        if (target == ScanTarget.PAIRING_CODE) {
            joinLauncher.error = message
        } else {
            launcher.error = message
            if (message == null) joinLauncher.error = null
        }
    }

    fun route(code: String) {
        routeScannedSetupCode(
            code = code,
            isPairingCode = isDevicePairingCode(code),
            onPairingCode = {
                launcher.cancel()
                launcher.error = null
                setShowJoin(true)
                // Fill the pairing field so the preview and provider check show
                // before the joiner commits to Join.
                joinLauncher.updatePairingCode(it, oauthLinking, oauthLinkingError)
            },
            onRestoreCode = {
                joinLauncher.reset()
                setShowJoin(false)
                launcher.link(it, oauthLinking, oauthLinkingError)
            },
        )
    }
}

internal fun routeScannedSetupCode(
    code: String,
    isPairingCode: Boolean,
    onPairingCode: (String) -> Unit,
    onRestoreCode: (String) -> Unit,
) {
    if (isPairingCode) {
        onPairingCode(code)
    } else {
        onRestoreCode(code)
    }
}
