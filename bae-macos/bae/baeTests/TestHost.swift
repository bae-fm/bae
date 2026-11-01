import BaeKit
import Foundation

/// A host for a test that hands the bridge a setup or an opening: over a
/// fresh bae directory under the temporary directory, with its keys and
/// credentials in an in-memory keychain, so no test reads or writes the
/// login keychain. One serves every test.
enum TestHost {
    static let shared: BridgeHost = {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("bae-test-\(UUID().uuidString)")
            .path
        do {
            return try BridgeHost.withTestKeyring(
                diagnostics: BaeDiagnostics.disabled(),
                appDir: BridgeAppDir(home: home)
            )
        }
        catch {
            preconditionFailure("The test host could not start: \(error)")
        }
    }()
}
