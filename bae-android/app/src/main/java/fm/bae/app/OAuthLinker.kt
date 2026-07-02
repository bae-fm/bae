package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeCloudProvider

/**
 * The OAuth client config + system-browser auth flow the onboarding code needs,
 * named without referencing the OAuth bridge bindings. Those bindings
 * (`oauthBegin`, `oauthComplete`, `setOauthClientCreds`) exist only in the
 * `full` edition; the implementation [OAuthLinking] lives in `src/full` and is
 * the sole thing that imports them. The `baeium` (S3-only) edition compiles
 * against bindings that lack the OAuth functions, so it has no implementation
 * and [load] returns null. Shared code holds an `OAuthLinker?` and, in baeium,
 * it is always null — the `needsOauth` restore path is unreachable for an S3
 * code, and if a token were somehow required the null surfaces a clear error.
 */
interface OAuthLinker {
    /**
     * Register the client ids with the bridge so coven can build authorization
     * URLs and refresh provider tokens during sync. Idempotent; call at launch.
     */
    fun register()

    /**
     * Run the OAuth flow for [provider] and return the token JSON to hand to
     * `restoreFromCode`.
     */
    suspend fun authorize(
        context: Context,
        provider: BridgeCloudProvider,
    ): String

    /**
     * Turn an OAuth token into the account email so the joiner can bake it into
     * its join-request. Routed through this interface because the underlying
     * `fetchAccountEmail` bridge binding exists only in the full edition.
     */
    suspend fun fetchAccountEmail(
        provider: BridgeCloudProvider,
        oauthTokenJson: String,
    ): String

    companion object {
        /**
         * Load the host's OAuth client creds, or null when this edition ships no
         * OAuth support (baeium) or no `assets/oauth-creds.json` is bundled (full
         * without configured credentials). Implemented per edition: the `full`
         * source set reads the creds file; the `baeium` source set always returns
         * null.
         */
        fun load(context: Context): OAuthLinker? = loadOAuthLinker(context)
    }
}
