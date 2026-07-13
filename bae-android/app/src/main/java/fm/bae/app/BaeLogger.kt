package fm.bae.app

import android.util.Log
import uniffi.bae_bridge.BridgeAppDiagnosticMetadata
import uniffi.bae_bridge.BridgeDatadogDiagnosticsConfig
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeDiagnosticsConfig
import uniffi.bae_bridge.configureDiagnostics

object BaeDiagnostics {
    /**
     * Construct the process-lifetime telemetry sink and install the core's
     * tracing subscriber. Call once at startup, before `initKeyring` and
     * `initApp`. Infallible: the core falls back to the no-op sink (with a
     * local error log) rather than let telemetry setup block a launch.
     */
    fun configure(): BridgeDiagnostics = configureDiagnostics(bridgeConfig())

    /**
     * Builds the Datadog telemetry config the sink is constructed from. Local
     * logging stays in [BaeLogger] (android.util.Log).
     */
    fun bridgeConfig(): BridgeDiagnosticsConfig {
        if (BuildConfig.BAE_EDITION == "bae") {
            val datadogSite = configuredBuildString(BuildConfig.BAE_DATADOG_SITE)
            val clientToken = configuredBuildString(BuildConfig.BAE_DATADOG_CLIENT_TOKEN)
            val environment = configuredBuildString(BuildConfig.BAE_ENVIRONMENT)
            if (datadogSite != null && clientToken != null && environment != null) {
                return BridgeDiagnosticsConfig.Enabled(
                    BridgeDatadogDiagnosticsConfig(
                        datadogSite = datadogSite,
                        clientToken = clientToken,
                        source = "android",
                        app =
                            BridgeAppDiagnosticMetadata(
                                service = "bae",
                                environment = environment,
                                appVersion = BuildConfig.VERSION_NAME,
                                edition = BuildConfig.BAE_EDITION,
                                gitCommit = BuildConfig.BAE_GIT_COMMIT,
                            ),
                    ),
                )
            }
        }

        return BridgeDiagnosticsConfig.Disabled
    }
}

internal fun configuredBuildString(value: String?): String? {
    val trimmed = value?.trim() ?: return null
    return trimmed.takeIf { it.isNotEmpty() && !it.startsWith("\$(") }
}

/**
 * Local, on-device logging. Writes to android.util.Log only — nothing here
 * ships to Datadog. Shipped telemetry is the typed catalog the Rust core emits;
 * host events go through `AppHandle.telemetry`.
 */
class BaeLogger(
    private val tag: String,
) {
    fun debug(
        message: String,
        throwable: Throwable? = null,
    ) {
        log(Log.DEBUG, message, throwable)
    }

    fun info(message: String) {
        log(Log.INFO, message, null)
    }

    fun warning(
        message: String,
        throwable: Throwable? = null,
    ) {
        log(Log.WARN, message, throwable)
    }

    fun error(
        message: String,
        throwable: Throwable? = null,
    ) {
        log(Log.ERROR, message, throwable)
    }

    private fun log(
        priority: Int,
        message: String,
        throwable: Throwable?,
    ) {
        Log.println(priority, tag, androidMessage(message, throwable))
    }

    private fun androidMessage(
        message: String,
        throwable: Throwable?,
    ): String =
        if (throwable == null) {
            message
        } else {
            "$message\n${Log.getStackTraceString(throwable)}"
        }
}

internal fun runLoggedBridgeCommand(
    logger: BaeLogger,
    operation: String,
    command: () -> Unit,
) {
    try {
        command()
    } catch (e: Exception) {
        logger.error("$operation failed", e)
    }
}
