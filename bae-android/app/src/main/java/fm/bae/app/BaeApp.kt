package fm.bae.app

import android.app.Application
import fm.bae.app.playback.AudioFocus
import io.crates.keyring.Keyring
import uniffi.bae_bridge.BridgeAppDir
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.initKeyring

private const val TAG = "bae.BaeApp"
private val logger = BaeLogger(TAG)

class BaeApp : Application() {
    /**
     * The process-lifetime telemetry sink, built at startup and held for the
     * whole app run. `initKeyring` and every library open require it; the open
     * session reaches it via `applicationContext as BaeApp`.
     */
    lateinit var diagnostics: BridgeDiagnostics
        private set

    /**
     * The process-lifetime host registrations, built at startup and held for
     * the whole app run. Every library open, restore, join, and OAuth sign-in
     * requires it; they reach it the same way as [diagnostics]. Unset only when
     * [platformStartupError] or [startupError] stopped the launch first, and
     * then nothing past the startup error screen runs.
     */
    lateinit var host: BridgeHost
        private set

    /**
     * The process's audio focus, which the host hands every library's playback
     * to ask before it starts. Built just before the host; the open session's
     * player follows its changes.
     */
    lateinit var audioFocus: AudioFocus
        private set

    var oauthProviders: OAuthProviders? = null
        private set

    var oauthProvidersError: String? = null
        private set

    var startupError: BridgeException? = null
        private set

    var platformStartupError: String? = null
        private set

    /** Bugs reported without crashing, for the whole run; the app's snackbar tells the person. */
    val bugs = BugReports()

    override fun onCreate() {
        super.onCreate()
        // Telemetry first, from compiled-in values only, so the sink exists for
        // every later launch step (crash reporter, keyring, library open) and
        // any failure it reports.
        diagnostics = BaeDiagnostics.configure()
        BaeCrashReporting.configure(this)
        logger.info("application launched")
        // Initialize the Android NDK context for the keyring store.
        // TLS initialization receives the same application context directly;
        // both must finish before any key or network operation.
        Keyring.initializeNdkContext(this)
        try {
            AndroidRuntime.initialize(this)
        } catch (error: RuntimeException) {
            platformStartupError = error.toString()
            logger.error("Failed to initialize Android TLS", error)
            return
        }
        // bae's directory lives under the app's private files dir: Android app
        // processes have no home directory of their own to put it under.
        // Building it creates it. The host's one failure is the OS refusing its
        // onboarding runtime's worker threads. Any of these failing stops the
        // launch at the startup error, and the exception names which one it was.
        try {
            audioFocus = AudioFocus(this)
            host = BridgeHost(diagnostics, BridgeAppDir(filesDir.absolutePath), audioFocus)
            initKeyring(diagnostics)
        } catch (error: BridgeException) {
            startupError = error
            logger.error("Failed to create bae's directory or start the bridge host or secure storage", error)
            return
        }
        // Register the app's OAuth clients and sign-in presenter (if a creds
        // file is bundled) so coven can sign in to a provider and refresh its
        // access during sync. Null in the baeium edition (no OAuth) or when no
        // creds file is bundled (full) → providers that need a sign-in stay
        // unavailable.
        try {
            oauthProviders = OAuthProviders.load(this)
            oauthProviders?.register(host)
        } catch (e: IllegalStateException) {
            oauthProvidersError = e.toString()
            logger.error("Failed to register OAuth clients", e)
        }
    }
}
