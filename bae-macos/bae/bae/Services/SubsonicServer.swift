import BaeKit
import Foundation

/// Subsonic server settings and credentials, used by the Subsonic settings tab.
/// The password is write-only from here (committed to the keyring); it is never
/// read back into observable state.
final class SubsonicServer: Sendable, Observable {
    let setServerConfig:
        @Sendable (_ enabled: Bool, _ portText: String, _ username: String)
            async throws -> Void
    let getServerStatus: @Sendable () async -> BridgeSubsonicServerStatus
    let setPassword: @Sendable (_ password: String) async throws -> Void

    init(
        setServerConfig:
            @escaping @Sendable (Bool, String, String) async throws -> Void,
        getServerStatus:
            @escaping @Sendable () async -> BridgeSubsonicServerStatus,
        setPassword: @escaping @Sendable (String) async throws -> Void
    ) {
        self.setServerConfig = setServerConfig
        self.getServerStatus = getServerStatus
        self.setPassword = setPassword
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            setServerConfig: {
                guard let port = UInt16($1), port > 0 else {
                    throw SubsonicValidationError.invalidPort
                }
                try handle.setSubsonicServerConfig(
                    enabled: $0,
                    port: port,
                    username: $2
                )
            },
            getServerStatus: { handle.getSubsonicServerStatus() },
            setPassword: { try handle.setSubsonicPassword(password: $0) }
        )
    }
}

private enum SubsonicValidationError: LocalizedError {
    case invalidPort

    var errorDescription: String? {
        switch self {
        case .invalidPort:
            String(localized: "Enter a port from 1 to 65535.")
        }
    }
}
