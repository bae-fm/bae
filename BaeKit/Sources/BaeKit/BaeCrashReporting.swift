import Foundation
import OSLog
import Sentry

private let logger = Logger.bae("BaeCrashReporting")

public enum BaeCrashReporting {
    public static func configure(edition: AppEdition) {
        // The baeium edition ships without crash reporting by design, so its
        // absence is expected and silent.
        guard edition == .bae,
            let config = config(
                dsn: BuildInfo.infoString("BaeSentryDsn"),
                environment: BuildInfo.infoString("BaeEnvironment"),
                appVersion: BuildInfo.infoString("CFBundleShortVersionString"),
                gitCommit: BuildInfo.infoString("BaeGitCommit"),
                warn: logger.warning
            )
        else { return }
        SentrySDK.start { options in
            options.dsn = config.dsn
            options.environment = config.environment
            options.releaseName = config.releaseName
            options.dist = config.gitCommit
            options.sendDefaultPii = false
        }
        SentrySDK.configureScope { scope in
            scope.setTag(value: config.edition, key: "edition")
            scope.setTag(value: config.gitCommit, key: "git_commit")
        }
    }

    /// The bae edition's crash-reporting config the build's values make, or
    /// nil — with a `warn`ing naming what was missing — when any is missing.
    static func config(
        dsn: String?,
        environment: String?,
        appVersion: String?,
        gitCommit: String?,
        warn: (String) -> Void
    ) -> CrashReportingConfig? {
        guard let dsn, let environment, let appVersion, let gitCommit else {
            let missing = [
                ("BaeSentryDsn", dsn),
                ("BaeEnvironment", environment),
                ("CFBundleShortVersionString", appVersion),
                ("BaeGitCommit", gitCommit),
            ]
            .filter { $0.1 == nil }
            .map(\.0)
            warn(
                "Crash reporting disabled: not configured at build time "
                    + "(missing \(missing.joined(separator: ", "))). "
                    + "Crashes will not be reported."
            )
            return nil
        }
        return CrashReportingConfig(
            dsn: dsn,
            environment: environment,
            releaseName: "bae@\(appVersion)+\(gitCommit)",
            edition: AppEdition.bae.rawValue,
            gitCommit: gitCommit
        )
    }
}

struct CrashReportingConfig {
    let dsn: String
    let environment: String
    let releaseName: String
    let edition: String
    let gitCommit: String
}
