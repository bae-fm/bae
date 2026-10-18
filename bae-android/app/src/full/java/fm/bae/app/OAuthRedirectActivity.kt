package fm.bae.app

import android.app.Activity
import android.os.Bundle

/**
 * Receives the OAuth provider redirect (custom scheme) from the system browser,
 * hands the complete redirect to the sign-in waiting in
 * [OAuthSignIn.pendingRedirect], then re-surfaces [MainActivity] so the person
 * returns to bae (popping the Custom Tab).
 */
class OAuthRedirectActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent?.data?.let { OAuthSignIn.pendingRedirect?.complete(it.toString()) }
        startActivity(mainActivityIntent(this))
        finish()
    }
}
