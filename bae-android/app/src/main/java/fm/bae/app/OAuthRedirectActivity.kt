package fm.bae.app

import android.app.Activity
import android.os.Bundle

/**
 * Receives the OAuth provider redirect (custom scheme) from the system browser,
 * hands the redirect Uri to the in-flight link flow via
 * [OAuthLinking.pendingRedirect], then re-surfaces [MainActivity] so the user
 * returns to bae (popping the Custom Tab) and finishes.
 */
class OAuthRedirectActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent?.data?.let { OAuthLinking.pendingRedirect?.complete(it) }
        startActivity(mainActivityIntent(this))
        finish()
    }
}
