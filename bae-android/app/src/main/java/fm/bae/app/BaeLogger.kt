package fm.bae.app

import android.util.Log
import uniffi.bae_bridge.BridgeAppDiagnosticMetadata
import uniffi.bae_bridge.BridgeDatadogDiagnosticsConfig
import uniffi.bae_bridge.BridgeDiagnosticField
import uniffi.bae_bridge.BridgeDiagnosticLevel
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeDiagnosticsConfig
import uniffi.bae_bridge.configureDiagnostics

private const val DIAGNOSTICS_TAG = "bae.Diagnostics"

object BaeDiagnostics {
    @Volatile
    private var diagnostics: BridgeDiagnostics? = null

    fun configure() {
        try {
            diagnostics = configureDiagnostics(bridgeConfig())
        } catch (e: Exception) {
            Log.e(DIAGNOSTICS_TAG, "Failed to configure diagnostics", e)
        }
    }

    fun log(
        level: BridgeDiagnosticLevel,
        target: String,
        message: String,
        fields: List<BridgeDiagnosticField> = emptyList(),
    ) {
        val configured = diagnostics ?: return
        try {
            configured.log(level, target, message, fields)
        } catch (e: Exception) {
            Log.d(DIAGNOSTICS_TAG, "Failed to send host diagnostic", e)
        }
    }

    private fun bridgeConfig(): BridgeDiagnosticsConfig {
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

class BaeLogger(
    private val tag: String,
) {
    fun debug(
        message: String,
        throwable: Throwable? = null,
    ) {
        log(BridgeDiagnosticLevel.DEBUG, message, throwable)
    }

    fun info(message: String) {
        log(BridgeDiagnosticLevel.INFO, message, null)
    }

    fun warning(
        message: String,
        throwable: Throwable? = null,
    ) {
        log(BridgeDiagnosticLevel.WARN, message, throwable)
    }

    fun error(
        message: String,
        throwable: Throwable? = null,
    ) {
        log(BridgeDiagnosticLevel.ERROR, message, throwable)
    }

    private fun log(
        level: BridgeDiagnosticLevel,
        message: String,
        throwable: Throwable?,
    ) {
        Log.println(androidPriority(level), tag, androidMessage(message, throwable))
        BaeDiagnostics.log(level, tag, diagnosticMessage(message, throwable))
    }

    private fun androidPriority(level: BridgeDiagnosticLevel): Int =
        when (level) {
            BridgeDiagnosticLevel.TRACE, BridgeDiagnosticLevel.DEBUG -> Log.DEBUG
            BridgeDiagnosticLevel.INFO -> Log.INFO
            BridgeDiagnosticLevel.WARN -> Log.WARN
            BridgeDiagnosticLevel.ERROR -> Log.ERROR
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

    private fun diagnosticMessage(
        message: String,
        throwable: Throwable?,
    ): String =
        if (throwable == null) {
            message
        } else {
            "$message: ${throwable::class.java.simpleName}: ${throwable.message}"
        }
}
