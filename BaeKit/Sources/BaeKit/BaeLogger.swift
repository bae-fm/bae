import OSLog

/// Local, on-device logging. Writes to OSLog only — nothing here ships to
/// Datadog. Shipped telemetry is the typed catalog the Rust core emits; host
/// events go through `AppHandle.telemetry`. Messages are public so Console
/// shows them outside a debugger.
public struct BaeLogger: Sendable {
    private let osLog: Logger

    fileprivate init(category: String) {
        osLog = Logger(subsystem: "fm.bae.desktop", category: category)
    }

    public func debug(_ message: String) {
        osLog.debug("\(message, privacy: .public)")
    }

    public func info(_ message: String) {
        osLog.info("\(message, privacy: .public)")
    }

    public func warning(_ message: String) {
        osLog.warning("\(message, privacy: .public)")
    }

    public func error(_ message: String) {
        osLog.error("\(message, privacy: .public)")
    }
}

extension Logger {
    public static func bae(_ category: String) -> BaeLogger {
        BaeLogger(category: category)
    }
}
