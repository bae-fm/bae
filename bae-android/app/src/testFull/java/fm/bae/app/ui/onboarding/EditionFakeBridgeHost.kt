package fm.bae.app.ui.onboarding

import fm.bae.app.playback.unfaked
import uniffi.bae_bridge.BridgeHostInterface
import uniffi.bae_bridge.BridgeOAuthClients
import uniffi.bae_bridge.BridgeOAuthRedirects
import uniffi.bae_bridge.OAuthPresenterCallback

/** The host calls only the full edition's bridge has (its cloud provider sign-in), which no test fakes. */
internal abstract class EditionFakeBridgeHost : BridgeHostInterface {
    override fun setOauthClients(clients: BridgeOAuthClients) {
        unfaked("setOauthClients")
    }

    override fun setOauthPresenter(
        redirects: BridgeOAuthRedirects,
        callback: OAuthPresenterCallback,
    ) {
        unfaked("setOauthPresenter")
    }
}
