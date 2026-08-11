/// Builds the Datadog telemetry config and constructs the process-lifetime
/// telemetry sink the app holds for its whole run. The sink is built once at
/// startup (from compiled-in values only) and required by `initKeyring` /
/// `initApp`, so telemetry exists before anything that could fail. Local logging
/// stays in `BaeLogger` (OSLog); only the typed config here, and the typed
/// events the core emits, ever reach Datadog.
public enum BaeDiagnostics {
    /// Construct the telemetry sink and install the core's tracing subscriber.
    /// Call once at startup, before `initKeyring` and `initApp`. Infallible:
    /// the core falls back to the no-op sink (with a local error log) rather
    /// than let telemetry setup block a launch.
    public static func configure(
        source: String,
        edition: AppEdition
    ) -> BridgeDiagnostics {
        configureDiagnostics(
            config: bridgeConfig(source: source, edition: edition)
        )
    }

    static func bridgeConfig(
        source: String,
        edition: AppEdition
    ) -> BridgeDiagnosticsConfig {
        guard edition == .bae,
            let datadogSite = BuildInfo.infoString("BaeDatadogSite"),
            let clientToken = BuildInfo.infoString("BaeDatadogClientToken"),
            let environment = BuildInfo.infoString("BaeEnvironment"),
            let appVersion = BuildInfo.infoString("CFBundleShortVersionString"),
            let gitCommit = BuildInfo.infoString("BaeGitCommit")
        else {
            return .disabled
        }

        return .enabled(
            config: BridgeDatadogDiagnosticsConfig(
                datadogSite: datadogSite,
                clientToken: clientToken,
                source: source,
                app: BridgeAppDiagnosticMetadata(
                    service: "bae",
                    environment: environment,
                    appVersion: appVersion,
                    edition: edition.rawValue,
                    gitCommit: gitCommit
                )
            )
        )
    }
}
