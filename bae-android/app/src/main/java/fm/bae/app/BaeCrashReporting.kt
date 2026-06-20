package fm.bae.app

import android.content.Context
import io.sentry.Sentry
import io.sentry.android.core.SentryAndroid

private const val CRASH_REPORTING_TAG = "bae.CrashReporting"
private val crashReportingLogger = BaeLogger(CRASH_REPORTING_TAG)

object BaeCrashReporting {
    fun configure(context: Context) {
        val config = config() ?: return
        try {
            SentryAndroid.init(context) { options ->
                options.dsn = config.dsn
                options.environment = config.environment
                options.release = "bae@${BuildConfig.VERSION_NAME}+${config.gitCommit}"
                options.dist = config.gitCommit
                options.isSendDefaultPii = false
                options.isEnableNdk = true
            }
            Sentry.configureScope { scope ->
                scope.setTag("edition", config.edition)
                scope.setTag("git_commit", config.gitCommit)
            }
        } catch (e: Exception) {
            crashReportingLogger.error("Failed to configure crash reporting", e)
        }
    }

    private fun config(): CrashReportingConfig? {
        var config: CrashReportingConfig? = null
        if (BuildConfig.BAE_EDITION == "bae") {
            val dsn = configuredBuildString(BuildConfig.BAE_SENTRY_DSN)
            val environment = configuredBuildString(BuildConfig.BAE_ENVIRONMENT)
            val gitCommit = configuredBuildString(BuildConfig.BAE_GIT_COMMIT)
            if (dsn != null && environment != null && gitCommit != null) {
                config =
                    CrashReportingConfig(
                        dsn = dsn,
                        environment = environment,
                        edition = BuildConfig.BAE_EDITION,
                        gitCommit = gitCommit,
                    )
            }
        }
        return config
    }
}

private data class CrashReportingConfig(
    val dsn: String,
    val environment: String,
    val edition: String,
    val gitCommit: String,
)
