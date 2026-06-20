package fm.bae.app

import android.content.Context
import android.net.Uri
import android.util.Log
import androidx.browser.customtabs.CustomTabsIntent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONException
import org.json.JSONObject
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.oauthBegin
import uniffi.bae_bridge.oauthComplete
import uniffi.bae_bridge.setOauthClientCreds
import java.io.FileNotFoundException
import java.io.IOException

private const val TAG = "bae.OAuthLinking"

/**
 * Load the host's OAuth client creds (full edition); null when no
 * `assets/oauth-creds.json` is bundled. The baeium edition defines its own
 * always-null `loadOAuthLinker`. [OAuthLinker.load] delegates here.
 */
fun loadOAuthLinker(context: Context): OAuthLinker? = OAuthLinking.load(context)

/**
 * Per-device OAuth client config + the system-browser auth flow for providers
 * that need it (Google Drive, Dropbox, OneDrive). Loaded from a gitignored
 * `assets/oauth-creds.json` — bae and coven ship no credentials; register your
 * own OAuth app's client id and the redirect URI from its console (an
 * installed-app client, so the redirect is a custom scheme). When the file is
 * absent, OAuth linking is unavailable and onboarding says so. See
 * notes/mobile-oauth.md.
 */
class OAuthLinking private constructor(
    private val providers: Map<String, ProviderConfig>,
) : OAuthLinker {
    data class ProviderConfig(
        val clientId: String,
        val clientSecret: String?,
        val redirectUri: String,
    )

    /**
     * Register the client ids with the bridge so coven can build authorization
     * URLs and refresh provider tokens during sync. Idempotent; call at launch.
     */
    override fun register() {
        val json = JSONObject()
        for ((provider, config) in providers) {
            val entry = JSONObject().put("client_id", config.clientId)
            config.clientSecret?.let { entry.put("client_secret", it) }
            json.put(provider, entry)
        }
        setOauthClientCreds(json.toString())
    }

    /**
     * Run the OAuth flow for [provider] and return the token JSON to hand to
     * `restoreFromCode`. Opens the system browser via a Custom Tab and awaits
     * the redirect captured by [OAuthRedirectActivity].
     */
    override suspend fun authorize(
        context: Context,
        provider: BridgeCloudProvider,
    ): String {
        val key =
            providerKey(provider)
                ?: throw IllegalStateException("Cloud sign-in isn't configured for this provider.")
        val config =
            providers[key]
                ?: throw IllegalStateException("Cloud sign-in isn't configured for this provider.")

        val request = oauthBegin(provider, config.redirectUri)

        val deferred = CompletableDeferred<Uri>()
        pendingRedirect = deferred
        try {
            CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(request.authUrl))
            val redirect = deferred.await()
            val code =
                redirect.getQueryParameter("code")
                    ?: throw IllegalStateException("Authorization finished without a code.")
            // The token exchange is a blocking network call (block_on in the
            // bridge), so keep it off the main thread.
            return withContext(Dispatchers.IO) {
                oauthComplete(provider, code, request.verifier, config.redirectUri)
            }
        } finally {
            pendingRedirect = null
        }
    }

    companion object {
        /**
         * Set while an auth flow awaits its redirect; completed by
         * [OAuthRedirectActivity] when the browser redirects back to the custom
         * scheme. A field rather than a registry because only one interactive
         * link flow runs at a time.
         */
        @Volatile
        var pendingRedirect: CompletableDeferred<Uri>? = null

        /** Load the bundled creds; null when no `assets/oauth-creds.json`. */
        fun load(context: Context): OAuthLinking? {
            val text =
                try {
                    context.assets
                        .open("oauth-creds.json")
                        .bufferedReader()
                        .use { it.readText() }
                } catch (e: FileNotFoundException) {
                    Log.d(TAG, "assets/oauth-creds.json not bundled; OAuth linking unavailable")
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
            val providers = mutableMapOf<String, ProviderConfig>()
            for (key in root.keys()) {
                val obj =
                    root.optJSONObject(key)
                        ?: throw IllegalStateException(
                            "oauth-creds.json entry for $key must be an object.",
                        )
                val clientId = requiredField(obj, key, "client_id")
                val redirectUri = requiredField(obj, key, "redirect_uri")
                val clientSecret = obj.optString("client_secret").takeIf { it.isNotEmpty() }
                providers[key] = ProviderConfig(clientId, clientSecret, redirectUri)
            }
            check(providers.isNotEmpty()) {
                "oauth-creds.json does not contain any provider credentials."
            }
            return OAuthLinking(providers)
        }

        private fun requiredField(
            obj: JSONObject,
            provider: String,
            field: String,
        ): String =
            obj.optString(field).takeIf { it.isNotEmpty() }
                ?: throw IllegalStateException(
                    "oauth-creds.json is missing $field for $provider.",
                )

        /** coven keys OAuth client creds by provider name. */
        private fun providerKey(provider: BridgeCloudProvider): String? =
            when (provider) {
                BridgeCloudProvider.GOOGLE_DRIVE -> "google_drive"

                BridgeCloudProvider.DROPBOX -> "dropbox"

                BridgeCloudProvider.ONE_DRIVE -> "onedrive"

                BridgeCloudProvider.S3,
                BridgeCloudProvider.CLOUD_KIT,
                -> null
            }
    }
}
