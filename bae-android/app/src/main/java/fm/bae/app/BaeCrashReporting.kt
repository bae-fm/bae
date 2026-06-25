package fm.bae.app

import android.content.Context
import io.sentry.Sentry
import io.sentry.android.core.SentryAndroid

private const val CRASH_REPORTING_TAG = "bae.CrashReporting"
private val crashReportingLogger = BaeLogger(CRASH_REPORTING_TAG)

object BaeCrashReporting {
    fun configure(context: Context) {
        // The baeium edition ships without crash reporting by design, so its
        // absence is expected and silent.
        if (BuildConfig.BAE_EDITION != "bae") return

        val dsn = configuredBuildString(BuildConfig.BAE_SENTRY_DSN)
        val environment = configuredBuildString(BuildConfig.BAE_ENVIRONMENT)
        val gitCommit = configuredBuildString(BuildConfig.BAE_GIT_COMMIT)
        if (dsn == null || environment == null || gitCommit == null) {
            crashReportingLogger.warning(
                "Crash reporting disabled: Sentry not configured at build time " +
                    "(need BAE_SENTRY_DSN, BAE_ENVIRONMENT, BAE_GIT_COMMIT). " +
                    "Crashes will not be reported.",
            )
            return
        }

        try {
            SentryAndroid.init(context) { options ->
                options.dsn = dsn
                options.environment = environment
                options.release = "bae@${BuildConfig.VERSION_NAME}+$gitCommit"
                options.dist = gitCommit
                options.isSendDefaultPii = false
                options.isEnableNdk = true
            }
            Sentry.configureScope { scope ->
                scope.setTag("edition", BuildConfig.BAE_EDITION)
                scope.setTag("git_commit", gitCommit)
            }
        } catch (e: Exception) {
            crashReportingLogger.error("Failed to configure crash reporting", e)
        }
    }
}
