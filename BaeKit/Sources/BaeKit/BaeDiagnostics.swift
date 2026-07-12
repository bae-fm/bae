import OSLog

public enum BaeDiagnostics {
    private static let osLog = Logger(
        subsystem: "fm.bae.desktop",
        category: "Diagnostics"
    )
    @MainActor
    private static var diagnostics: BridgeDiagnostics?

    @MainActor
    public static func configure(source: String) {
        do {
            diagnostics = try configureDiagnostics(
                config: bridgeConfig(source: source)
            )
        }
        catch {
            osLog.error(
                "Failed to configure diagnostics: \(error.localizedDescription)"
            )
        }
    }

    public static func log(
        level: BridgeDiagnosticLevel,
        target: String,
        message: String
    ) {
        Task { @MainActor in
            guard let diagnostics else { return }
            do {
                try diagnostics.log(
                    level: level,
                    target: target,
                    message: message,
                    fields: []
                )
            }
            catch {
                osLog.debug(
                    "Failed to send host diagnostic: \(error.localizedDescription)"
                )
            }
        }
    }

    @MainActor
    private static func bridgeConfig(
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
