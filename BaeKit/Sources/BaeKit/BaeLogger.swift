import OSLog

public struct BaeLogger: Sendable {
    private let osLog: Logger
    private let target: String

    fileprivate init(category: String) {
        osLog = Logger(subsystem: "fm.bae.desktop", category: category)
        target = category
    }

    public func debug(_ message: String) {
        log(message, level: .debug)
    }

    public func info(_ message: String) {
        log(message, level: .info)
    }

    public func warning(_ message: String) {
        log(message, level: .warn)
    }

    public func error(_ message: String) {
        log(message, level: .error)
    }

    private func log(
        _ message: String,
        level: BridgeDiagnosticLevel,
        fields: [BridgeDiagnosticField]? = nil
    ) {
        switch level {
        case .trace, .debug:
            osLog.debug("\(message)")
        case .info:
            osLog.info("\(message)")
        case .warn:
            osLog.warning("\(message)")
        case .error:
            osLog.error("\(message)")
        }
        BaeDiagnostics.log(
            level: level,
            target: target,
            message: message,
            fields: fields
        )
    }
}

extension Logger {
    public static func bae(_ category: String) -> BaeLogger {
        BaeLogger(category: category)
    }
}
