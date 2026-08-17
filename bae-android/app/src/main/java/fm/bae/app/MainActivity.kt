package fm.bae.app

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.lifecycleScope
import fm.bae.app.ui.ContentView
import kotlinx.coroutines.launch

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

    // The app shortcut (res/xml/shortcuts.xml) that launched or re-entered this
    // Activity, if any. Compose reads it to drive the one-shot open action
    // (resume playback / open search); the UI clears it once handled so a later
    // library switch doesn't replay it. singleTop (manifest) routes a warm
    // shortcut tap to onNewIntent, so both entry points feed this state.
    private var pendingShortcut by mutableStateOf<ShortcutAction?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        pendingShortcut = shortcutActionFromIntent(intent)
        val app = application as BaeApp
        setContent {
            ContentView(
                oauthLinking = app.oauthLinking,
                oauthLinkingError = app.oauthLinkingError,
                startupError =
                    app.platformStartupError
                        ?: app.startupError?.let { localizedLine(it) ?: it.toString() },
                shortcutAction = pendingShortcut,
                onShortcutHandled = { pendingShortcut = null },
            )
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        // A relaunch without a shortcut extra (e.g. the launcher icon) leaves any
        // still-pending shortcut alone rather than clearing it mid-handle.
        shortcutActionFromIntent(intent)?.let { pendingShortcut = it }
    }

    override fun onStop() {
        super.onStop()
        // Persist playback so the queue, current track, and position survive
        // process death while backgrounded. We can't shut core down (that would
        // stop the background audio), so this is the save point on Android.
        val session = AppSessionHolder.currentSession() ?: return
        lifecycleScope.launch {
            performBridgeAction(
                logger = logger,
                operation = "save playback state",
                errors = LocaleErrorLines(this@MainActivity),
                showError = session.configStore::showError,
            ) {
                session.appHandle.savePlaybackState()
            }
        }
    }
}
