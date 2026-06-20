import Foundation
import Sentry

enum BaeCrashReporting {
    static func configure() {
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
        guard edition == "bae",
            let dsn = infoString("BaeSentryDsn"),
            let environment = infoString("BaeEnvironment"),
            let appVersion = infoString("CFBundleShortVersionString"),
            let gitCommit = infoString("BaeGitCommit")
        else {
            return nil
        }
        return CrashReportingConfig(
            dsn: dsn,
            environment: environment,
            releaseName: "bae@\(appVersion)+\(gitCommit)",
            edition: edition,
            gitCommit: gitCommit
        )
    }

    private static var edition: String {
        #if BAE_OAUTH_PROVIDERS
            "bae"
        #else
            "baeium"
        #endif
    }

    private static func infoString(_ key: String) -> String? {
        guard let value = Bundle.main.infoDictionary?[key] as? String else {
            return nil
        }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty || trimmed.hasPrefix("$(") {
            return nil
        }
        return trimmed
    }
}

private struct CrashReportingConfig {
    let dsn: String
    let environment: String
    let releaseName: String
    let edition: String
    let gitCommit: String
}
