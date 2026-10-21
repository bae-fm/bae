package fm.bae.app.ui

import fm.bae.app.OAuthProviders
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeLibrary

/** What screens inside an open library ask of the app around it. */
internal class SessionActions(
    /** The provider sign-in this edition ships, null when it ships none. */
    val oauthProviders: OAuthProviders?,
    val switchLibrary: (BridgeLibrary) -> Unit,
    /** Remove the open library from this device, once the person confirmed. */
    val removeLibrary: () -> Unit,
    /** Reset this device for the open library, once the person accepted the reset's warning. */
    val resetDevice: (BridgeDeviceReset) -> Unit,
    /** Close the open library and ask for the person's restore code to restore it over this copy. */
    val restoreFromCode: () -> Unit,
    /** List this device's libraries again, after the last listing failed. */
    val reloadLibraries: () -> Unit,
)
