import Foundation
import Sentry

public enum BaeCrashReporting {
    public static func configure() {
        guard let config = config() else { return }
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

    private static func config() -> CrashReportingConfig? {
        guard BuildInfo.edition == "bae",
            let dsn = BuildInfo.infoString("BaeSentryDsn"),
            let environment = BuildInfo.infoString("BaeEnvironment"),
            let appVersion = BuildInfo.infoString("CFBundleShortVersionString"),
            let gitCommit = BuildInfo.infoString("BaeGitCommit")
        else {
            return nil
        }
        return CrashReportingConfig(
            dsn: dsn,
            environment: environment,
            releaseName: "bae@\(appVersion)+\(gitCommit)",
            edition: BuildInfo.edition,
            gitCommit: gitCommit
        )
    }
}

private struct CrashReportingConfig {
    let dsn: String
    let environment: String
    let releaseName: String
    let edition: String
    let gitCommit: String
}
