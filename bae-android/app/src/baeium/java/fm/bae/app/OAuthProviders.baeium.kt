package fm.bae.app

import android.content.Context

/**
 * The baeium (S3-only) edition ships no account providers: its bridge bindings
 * lack the OAuth functions, so there is no [OAuthSignIn] to construct and no
 * provider that needs a sign-in is offered.
 */
fun loadOAuthProviders(context: Context): OAuthProviders? = null
