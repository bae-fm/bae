import BaeKit
import Foundation

/// Automation server settings and credentials, used by the Automation settings
/// tab. Token values are returned only for copy/rotate actions and are not kept
/// in observable state.
final class Automation: Sendable, Observable {
    let setMcpServerConfig:
        @Sendable (_ enabled: Bool, _ portText: String) async throws -> Void
    let getMcpServerStatus: @Sendable () async -> BridgeMcpServerStatus
    let getMcpToken: @Sendable () async throws -> String
    let generateMcpToken: @Sendable () async throws -> String
    let setMcpToken: @Sendable (_ token: String) async throws -> Void

    init(
        setMcpServerConfig:
            @escaping @Sendable (Bool, String) async throws -> Void,
        getMcpServerStatus:
            @escaping @Sendable () async ->
            BridgeMcpServerStatus,
        getMcpToken: @escaping @Sendable () async throws -> String,
        generateMcpToken: @escaping @Sendable () async throws -> String,
        setMcpToken: @escaping @Sendable (String) async throws -> Void
    ) {
        self.setMcpServerConfig = setMcpServerConfig
        self.getMcpServerStatus = getMcpServerStatus
        self.getMcpToken = getMcpToken
        self.generateMcpToken = generateMcpToken
        self.setMcpToken = setMcpToken
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            setMcpServerConfig: {
                guard let port = UInt16($1), port > 0 else {
                    throw AutomationValidationError.invalidPort
                }
                try handle.setMcpServerConfig(enabled: $0, port: port)
            },
            getMcpServerStatus: { handle.getMcpServerStatus() },
            getMcpToken: { try handle.getMcpToken() },
            generateMcpToken: { handle.generateMcpToken() },
            setMcpToken: { try handle.setMcpToken(token: $0) }
        )
    }
}

private enum AutomationValidationError: LocalizedError {
    case invalidPort

    var errorDescription: String? {
        switch self {
        case .invalidPort:
            String(localized: "Enter a port from 1 to 65535.")
        }
    }
}
