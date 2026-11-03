import BaeKit
import Foundation

/// The host a test hands the bridge a setup or an opening over: a fresh bae
/// directory in a temporary folder, with its keys and credentials in an
/// in-memory keychain, so no test reads or writes the login keychain. Each
/// test makes its own, and the folder goes when the host does.
enum TestHost {
    static func make() -> BridgeHost {
        do {
            return try BridgeHost.withTestKeyring(
                diagnostics: BaeDiagnostics.disabled(),
                appDir: BridgeAppDir.temporary()
            )
        }
        catch {
            preconditionFailure("The test host could not start: \(error)")
        }
    }
}
