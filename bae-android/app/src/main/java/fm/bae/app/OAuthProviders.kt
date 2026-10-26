package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeStorageFolder
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup

/**
 * Sign-in to the account providers (Dropbox, OneDrive) and the storage setup
 * that needs it, named without referencing the OAuth bridge bindings. Those
 * bindings exist only in the `full` edition; its implementation `OAuthSignIn`
 * in `src/full` is the one thing that imports them. The `baeium` (S3-only)
 * edition compiles against bindings without them, so it has no implementation
 * and [load] returns null: shared code holds an `OAuthProviders?`, and in
 * baeium no account provider is offered or can be asked for.
 */
interface OAuthProviders {
    /**
     * Register the app's client ids, and the Custom Tab sign-in coven presents
     * when a provider needs the person to sign in, with [host]. Call at launch.
     */
    fun register(host: BridgeHost)

    /** Sign the open library's device in to [provider] before choosing a folder on it. */
    suspend fun signIn(
        handle: AppHandleInterface,
        provider: BridgeCloudProvider,
    )

    /**
     * Sign in again to the open library's provider when its sync asks, with the
     * access its location needs, and reconnect that location under [deviceName].
     */
    suspend fun signInAgain(
        handle: AppHandleInterface,
        deviceName: String,
    ): BridgeStorageSetup

    /** The Dropbox folders shared with the signed-in account, each a place a library can live. */
    suspend fun dropboxSharedFolders(handle: AppHandleInterface): List<BridgeStorageFolder>

    /** The top of the signed-in OneDrive. */
    suspend fun oneDriveRoot(handle: AppHandleInterface): BridgeStorageFolder

    /** The folders inside [parent] on the signed-in OneDrive. */
    suspend fun oneDriveFolders(
        handle: AppHandleInterface,
        parent: BridgeStorageLocation,
    ): List<BridgeStorageFolder>

    /** A new folder named [name] inside [parent] on the signed-in OneDrive. */
    suspend fun createOneDriveFolder(
        handle: AppHandleInterface,
        parent: BridgeStorageLocation,
        name: String,
    ): BridgeStorageFolder

    /** Make [location] the open library's cloud home. */
    suspend fun setUpStorage(
        handle: AppHandleInterface,
        location: BridgeStorageLocation,
        deviceName: String,
    ): BridgeStorageSetup

    companion object {
        /**
         * The provider sign-in this edition ships, or null when it ships none
         * (baeium) or when no `assets/oauth-creds.json` is bundled (full
         * without configured credentials). Implemented per edition.
         */
        fun load(context: Context): OAuthProviders? = loadOAuthProviders(context)
    }
}
