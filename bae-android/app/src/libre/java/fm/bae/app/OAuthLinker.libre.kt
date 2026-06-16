package fm.bae.app

import android.content.Context

/**
 * The libre (S3-only) edition ships no OAuth support: its bridge bindings lack
 * the OAuth functions, so there is no [OAuthLinking] to construct. Onboarding
 * holds the resulting null and never reaches the `needsOauth` branch — an S3
 * restore code never asks for a token.
 */
fun loadOAuthLinker(context: Context): OAuthLinker? = null
