package fm.bae.app

import android.app.Application
import io.crates.keyring.Keyring
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.initKeyring
import uniffi.bae_bridge.setCaCertDir
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

    var oauthLinking: OAuthLinker? = null
        private set

    var oauthLinkingError: String? = null
        private set

    var startupError: BridgeException? = null
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
        // The TLS stack (S3 + reqwest, via rustls-native-certs) can't find CA
        // roots on Android's default POSIX probe paths, so cloud connections
        // fail to verify. Point it at the OS trust store so the platform owns
        // CA trust + updates. conscrypt's dir is the Play-updatable one (API
        // 30+); the /system dir is the fallback on older devices.
        setCaCertDir("/apex/com.android.conscrypt/cacerts:/system/etc/security/cacerts")
        // Initialize the Android NDK context for the keyring store.
        // Must be called before initKeyring() so the Rust side has
        // access to the Android Context for SharedPreferences / KeyStore.
        Keyring.initializeNdkContext(this)
        try {
            initKeyring(diagnostics)
        } catch (error: BridgeException) {
            startupError = error
            logger.error("Failed to initialize secure storage", error)
            return
        }
        // Register the host's OAuth client creds (if a creds file is bundled) so
        // coven can build authorization URLs and refresh provider tokens during
        // sync. Null in the baeium edition (no OAuth) or when no creds file is
        // bundled (full) → cloud providers that need OAuth stay unavailable.
        try {
            oauthLinking = OAuthLinker.load(this)
            oauthLinking?.register()
        } catch (e: Exception) {
            oauthLinkingError = e.toString()
            logger.error("Failed to register OAuth client creds", e)
        }
    }
}
