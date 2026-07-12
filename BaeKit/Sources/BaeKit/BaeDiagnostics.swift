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
        guard edition == "bae",
            let datadogSite = infoString("BaeDatadogSite"),
            let clientToken = infoString("BaeDatadogClientToken"),
            let environment = infoString("BaeEnvironment"),
            let appVersion = infoString("CFBundleShortVersionString"),
            let gitCommit = infoString("BaeGitCommit")
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
                    edition: edition,
                    gitCommit: gitCommit
                )
            )
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
