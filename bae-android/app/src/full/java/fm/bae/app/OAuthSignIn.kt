package fm.bae.app

import android.content.Context
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONException
import org.json.JSONObject
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeOAuthClients
import uniffi.bae_bridge.BridgeOAuthPresentationException
import uniffi.bae_bridge.BridgeOAuthRedirects
import uniffi.bae_bridge.BridgeStorageFolder
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.OAuthPresenterCallback
import java.io.FileNotFoundException
import java.io.IOException

private const val TAG = "bae.OAuthSignIn"
private val logger = BaeLogger(TAG)

/**
 * Load the app's OAuth registrations (full edition); null when no
 * `assets/oauth-creds.json` is bundled. The baeium edition defines its own
 * always-null `loadOAuthProviders`. [OAuthProviders.load] delegates here.
 */
fun loadOAuthProviders(context: Context): OAuthProviders? = OAuthSignIn.load(context)

/**
 * The app's OAuth registrations and the Custom Tab sign-in coven presents.
 * Read from a gitignored `assets/oauth-creds.json` — bae and coven ship no
 * credentials. The file has one entry per provider a phone signs in to,
 * `dropbox` and `onedrive`, each with the `redirect_uri` registered for this
 * app (a custom scheme, which the manifest routes to [OAuthRedirectActivity])
 * and, for a provider this build offers, its `client_id`. A provider without a
 * client id is not registered, and coven says so when it is asked for.
 */
class OAuthSignIn private constructor(
    private val clients: BridgeOAuthClients,
    private val redirects: BridgeOAuthRedirects,
) : OAuthProviders,
    OAuthPresenterCallback {
    override fun register(host: BridgeHost) {
        host.setOauthClients(clients)
        host.setOauthPresenter(redirects, this)
    }

    /**
     * Show [authorizationUrl] in a Custom Tab over the foreground activity and
     * return the redirect the provider sends back. Closing the tab cancels the
     * sign-in; coven cancelling it brings bae back over the tab.
     */
    override suspend fun present(authorizationUrl: String): String =
        withContext(Dispatchers.Main) {
            val activity =
                ForegroundActivity.get()
                    ?: throw BridgeOAuthPresentationException.Failed("bae is not in the foreground to show the sign-in")
            val redirect = CompletableDeferred<String>()
            pendingRedirect = redirect
            // bae returning to the front without a redirect means the person closed the tab.
            val removeListener =
                ForegroundActivity.onResume {
                    redirect.completeExceptionally(BridgeOAuthPresentationException.Cancelled())
                }
            try {
                CustomTabsIntent.Builder().build().launchUrl(activity, Uri.parse(authorizationUrl))
                redirect.await()
            } catch (e: CancellationException) {
                activity.startActivity(mainActivityIntent(activity))
                throw e
            } finally {
                removeListener()
                pendingRedirect = null
            }
        }

    override suspend fun signIn(
        handle: AppHandle,
        provider: BridgeCloudProvider,
    ) {
        handle.signIn(provider)
    }

    override suspend fun signInAgain(
        handle: AppHandle,
        deviceName: String,
    ): BridgeStorageSetup = handle.signInAgain(deviceName)

    override suspend fun dropboxSharedFolders(handle: AppHandle): List<BridgeStorageFolder> = handle.dropboxSharedFolders()

    override suspend fun oneDriveRoot(handle: AppHandle): BridgeStorageFolder = handle.onedriveRoot()

    override suspend fun oneDriveFolders(
        handle: AppHandle,
        parent: BridgeStorageLocation,
    ): List<BridgeStorageFolder> = handle.onedriveFolders(parent)

    override suspend fun createOneDriveFolder(
        handle: AppHandle,
        parent: BridgeStorageLocation,
        name: String,
    ): BridgeStorageFolder = handle.createOnedriveFolder(parent, name)

    override suspend fun setUpStorage(
        handle: AppHandle,
        location: BridgeStorageLocation,
        deviceName: String,
    ): BridgeStorageSetup = handle.setupOauthStorage(location, deviceName)

    companion object {
        /**
         * Set while a sign-in awaits its redirect; completed by
         * [OAuthRedirectActivity] when the browser redirects back to the
         * custom scheme. A field rather than a registry because coven runs one
         * sign-in at a time.
         */
        @Volatile
        var pendingRedirect: CompletableDeferred<String>? = null

        /** Load the bundled registrations; null when no `assets/oauth-creds.json`. */
        fun load(context: Context): OAuthSignIn? {
            val text =
                try {
                    context.assets
                        .open("oauth-creds.json")
                        .bufferedReader()
                        .use { it.readText() }
                } catch (e: FileNotFoundException) {
                    logger.debug("assets/oauth-creds.json not bundled; provider sign-in unavailable")
                    return null
                } catch (e: IOException) {
                    throw IllegalStateException("Couldn't read oauth-creds.json: ${e.message}", e)
                }
            val root =
                try {
                    JSONObject(text)
                } catch (e: JSONException) {
                    throw IllegalStateException("oauth-creds.json is malformed: ${e.message}", e)
                }
            val dropbox = provider(root, "dropbox")
            val oneDrive = provider(root, "onedrive")
            return OAuthSignIn(
                clients =
                    BridgeOAuthClients(
                        dropboxAppKey = dropbox.clientId,
                        onedriveClientId = oneDrive.clientId,
                    ),
                redirects =
                    BridgeOAuthRedirects(
                        dropbox = dropbox.redirectUri,
                        onedrive = oneDrive.redirectUri,
                    ),
            )
        }

        private class ProviderEntry(
            val clientId: String?,
            val redirectUri: String,
        )

        private fun provider(
            root: JSONObject,
            key: String,
        ): ProviderEntry {
            val entry =
                root.optJSONObject(key)
                    ?: throw IllegalStateException("oauth-creds.json has no $key entry.")
            val redirectUri =
                entry.optString("redirect_uri").takeIf { it.isNotEmpty() }
                    ?: throw IllegalStateException("oauth-creds.json is missing redirect_uri for $key.")
            return ProviderEntry(
                clientId = entry.optString("client_id").takeIf { it.isNotEmpty() },
                redirectUri = redirectUri,
            )
        }
    }
}
