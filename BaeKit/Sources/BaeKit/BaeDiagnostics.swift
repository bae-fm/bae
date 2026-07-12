/// Builds the Datadog telemetry config the app hands to `initApp`. Telemetry is
/// constructed inside the Rust core from this config — there is no separate
/// configure step to run first. Local logging stays in `BaeLogger` (OSLog);
/// only this typed config, and the typed events the core emits, ever reach
/// Datadog.
public enum BaeDiagnostics {
    public static func bridgeConfig(
        source: String
    ) -> BridgeDiagnosticsConfig {
        guard BuildInfo.edition == "bae",
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
                    edition: BuildInfo.edition,
                    gitCommit: gitCommit
                )
            )
        )
    }
}
