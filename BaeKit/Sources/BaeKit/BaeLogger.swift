import OSLog

/// Local, on-device logging. Writes to OSLog only — nothing here ships to
/// Datadog. Shipped telemetry is the typed catalog the Rust core emits; host
/// events go through `AppHandle.telemetry`.
public struct BaeLogger: Sendable {
    private let osLog: Logger

    fileprivate init(category: String) {
        osLog = Logger(subsystem: "fm.bae.desktop", category: category)
    }

    public func debug(_ message: String) {
        osLog.debug("\(message)")
    }

    public func info(_ message: String) {
        osLog.info("\(message)")
    }

    public func warning(_ message: String) {
        osLog.warning("\(message)")
    }

    public func error(_ message: String) {
        osLog.error("\(message)")
    }
}

extension Logger {
    public static func bae(_ category: String) -> BaeLogger {
        BaeLogger(category: category)
    }
}
