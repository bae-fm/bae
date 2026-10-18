/// Builds the process-lifetime `BridgeHost` the app holds for its whole run:
/// the object that carries bae's directory, the OAuth applications, and the
/// runtime that create, restore, join and sign-in run on.
public enum BaeHost {
    /// Build the host around the telemetry sink and bae's directory. Call once
    /// at startup, right after `BaeDiagnostics.configure`. Its one failure is
    /// the OS refusing the onboarding runtime's worker threads at launch, which
    /// leaves the app unable to restore, join, sign in, or open a library, so it
    /// stops the launch with the reason.
    public static func make(
        diagnostics: BridgeDiagnostics,
        appDir: BridgeAppDir
    ) -> BridgeHost {
        make { try BridgeHost(diagnostics: diagnostics, appDir: appDir) }
    }

    /// Build the host with `construct`, such as a debug app's host whose
    /// keychain stays in memory for its UI tests; it fails as `make` does.
    public static func make(
        _ construct: () throws -> BridgeHost
    ) -> BridgeHost {
        do {
            return try construct()
        }
        catch {
            preconditionFailure(
                "The host's onboarding runtime could not start: \(error)"
            )
        }
    }
}
