package fm.bae.app

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import fm.bae.app.ui.ContentView

private const val TAG = "bae.MainActivity"
private val logger = BaeLogger(TAG)

internal fun mainActivityIntent(context: Context): Intent =
    Intent(context, MainActivity::class.java).apply {
        addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
    }

class MainActivity : ComponentActivity() {
    // Android 13+ requires this runtime permission for the media playback
    // notification to show. Playback works without it, but lock-screen /
    // Bluetooth / Android Auto controls only appear once it's granted. Launching
    // is a no-op after the first decision, so it's safe to fire on every launch.
    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            logger.info("POST_NOTIFICATIONS granted=$granted")
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        val app = application as BaeApp
        setContent {
            ContentView(
                oauthLinking = app.oauthLinking,
                oauthLinkingError = app.oauthLinkingError,
            )
        }
    }

    override fun onStop() {
        super.onStop()
        // Persist playback so the queue, current track, and position survive
        // process death while backgrounded. We can't shut core down (that would
        // stop the background audio), so this is the save point on Android.
        AppSessionHolder.currentSession()?.appHandle?.savePlaybackState()
    }
}
