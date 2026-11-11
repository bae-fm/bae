import BaeKit
import Foundation

/// Subsonic server settings and credentials, used by the Subsonic settings tab.
/// The password is write-only from here (committed to the keyring); it is never
/// read back into observable state.
final class SubsonicServer: Sendable, Observable {
    let setEnabled: @Sendable (_ enabled: Bool) async throws -> Void
    let setPort: @Sendable (_ port: UInt32) async throws -> Void
    let setUsername: @Sendable (_ username: String) async throws -> Void
    let setNetwork:
        @Sendable (_ network: BridgeSubsonicNetwork) async throws -> Void
    let setPassword: @Sendable (_ password: String) async throws -> Void

    init(
        setEnabled: @escaping @Sendable (Bool) async throws -> Void,
        setPort: @escaping @Sendable (UInt32) async throws -> Void,
        setUsername: @escaping @Sendable (String) async throws -> Void,
        setNetwork:
            @escaping @Sendable (BridgeSubsonicNetwork) async throws ->
            Void,
        setPassword: @escaping @Sendable (String) async throws -> Void
    ) {
        self.setEnabled = setEnabled
        self.setPort = setPort
        self.setUsername = setUsername
        self.setNetwork = setNetwork
        self.setPassword = setPassword
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            setEnabled: { try await handle.setSubsonicEnabled(enabled: $0) },
            setPort: { try await handle.setSubsonicPort(port: $0) },
            setUsername: { try await handle.setSubsonicUsername(username: $0) },
            setNetwork: { try await handle.setSubsonicNetwork(network: $0) },
            setPassword: { try await handle.setSubsonicPassword(password: $0) }
        )
    }
}
