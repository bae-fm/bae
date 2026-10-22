import Testing

@testable import BaeKit

/// Crash reporting needs every build value; a build missing one leaves it off
/// and says so in the log, naming what was missing.
@Suite("Crash-reporting config")
struct BaeCrashReportingTests {
    @Test("every value present makes the config")
    func everyValuePresentMakesTheConfig() {
        var warnings: [String] = []

        let config = BaeCrashReporting.config(
            dsn: "https://key@sentry.example/1",
            environment: "production",
            appVersion: "1.2.3",
            gitCommit: "abc1234",
            warn: { warnings.append($0) }
        )

        #expect(config?.releaseName == "bae@1.2.3+abc1234")
        #expect(warnings.isEmpty)
    }

    @Test("a missing value leaves crash reporting off and warns which")
    func aMissingValueWarnsWhich() {
        var warnings: [String] = []

        let config = BaeCrashReporting.config(
            dsn: nil,
            environment: "production",
            appVersion: "1.2.3",
            gitCommit: nil,
            warn: { warnings.append($0) }
        )

        #expect(config == nil)
        #expect(warnings.count == 1)
        #expect(warnings.first?.contains("BaeSentryDsn, BaeGitCommit") == true)
    }
}
