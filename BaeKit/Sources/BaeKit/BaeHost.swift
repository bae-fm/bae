/// Builds the process-lifetime `BridgeHost` the app holds for its whole run:
/// the object that carries the OAuth client registration, the CloudKit driver,
/// the in-flight OAuth sign-in, and the runtime restore, join, and sign-in run
/// on.
public enum BaeHost {
    /// Build the host around the telemetry sink. Call once at startup, right
    /// after `BaeDiagnostics.configure`. Its one failure is the OS refusing the
    /// onboarding runtime's worker threads at launch, which leaves the app
    /// unable to restore, join, sign in, or open a library, so it stops the
    /// launch with the reason.
    public static func make(diagnostics: BridgeDiagnostics) -> BridgeHost {
        do {
            return try BridgeHost(diagnostics: diagnostics)
        }
        catch {
            preconditionFailure(
                "The host's onboarding runtime could not start: \(error)"
            )
        }
    }
}
