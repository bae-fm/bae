package fm.bae.app

import android.app.Application
import io.crates.keyring.Keyring
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.initKeyring
import uniffi.bae_bridge.setDataDir

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

    var oauthLinking: OAuthLinker? = null
        private set

    var oauthLinkingError: String? = null
        private set

    var startupError: BridgeException? = null
        private set

    var platformStartupError: String? = null
        private set

    override fun onCreate() {
        super.onCreate()
        // Telemetry first, from compiled-in values only, so the sink exists for
        // every later launch step (crash reporter, keyring, library open) and
        // any failure it reports.
        diagnostics = BaeDiagnostics.configure()
        BaeCrashReporting.configure(this)
        logger.info("application launched")
        // Android app processes have no $HOME, which bae-core needs to locate
        // its data root (`~/.bae`). Point it at our private files dir before any
        // library access (discover/restore/initApp) so those don't fail with
        // "could not determine home directory".
        setDataDir(filesDir.absolutePath)
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
        // The host's one failure is the OS refusing its onboarding runtime's
        // worker threads; either failure stops the launch at the startup
        // error, and the exception names which one it was.
        try {
            host = BridgeHost(diagnostics)
            initKeyring(diagnostics)
        } catch (error: BridgeException) {
            startupError = error
            logger.error("Failed to start the bridge host or secure storage", error)
            return
        }
        // Register the host's OAuth client creds (if a creds file is bundled) so
        // coven can build authorization URLs and refresh provider tokens during
        // sync. Null in the baeium edition (no OAuth) or when no creds file is
        // bundled (full) → cloud providers that need OAuth stay unavailable.
        try {
            oauthLinking = OAuthLinker.load(this)
            oauthLinking?.register(host)
        } catch (e: Exception) {
            oauthLinkingError = e.toString()
            logger.error("Failed to register OAuth client creds", e)
        }
    }
}
