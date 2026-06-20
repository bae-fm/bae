import OSLog

struct BaeLogger {
    private let osLog: Logger
    private let target: String

    fileprivate init(category: String) {
        osLog = Logger(subsystem: "fm.bae.desktop", category: category)
        target = category
    }

    func debug(_ message: String) {
        log(message, level: .debug)
    }

    func info(_ message: String) {
        log(message, level: .info)
    }

    func warning(_ message: String) {
        log(message, level: .warn)
    }

    func error(_ message: String) {
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
    static func bae(_ category: String) -> BaeLogger {
        BaeLogger(category: category)
    }
}
