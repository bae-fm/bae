package fm.bae.app.ui.settings

import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeJoinRequest

/** A sent invite's request waiting for the admin to approve or decline; null otherwise. */
internal val BridgeInvite.requestToDecide: BridgeJoinRequest?
    get() = ((shown as? BridgeInviteShown.Code)?.progress as? BridgeInviteProgress.Deciding)?.request
