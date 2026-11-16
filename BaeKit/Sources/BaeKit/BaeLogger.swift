import Foundation
import OSLog

/// Local, on-device logging. Writes to OSLog only — nothing here ships to
/// Datadog. Shipped telemetry is the typed catalog the Rust core emits; host
/// events go through `AppHandle.telemetry`. Messages are public so Console
/// shows them outside a debugger.
public struct BaeLogger: Sendable {
    private let osLog: Logger
    private let category: String

    fileprivate init(category: String) {
        self.category = category
        osLog = Logger(subsystem: "fm.bae.desktop", category: category)
    }

    public func debug(_ message: String) {
        osLog.debug("\(message, privacy: .public)")
        launchLogFile?.append("debug", category, message)
    }

    public func info(_ message: String) {
        osLog.info("\(message, privacy: .public)")
        launchLogFile?.append("info", category, message)
    }

    public func warning(_ message: String) {
        osLog.warning("\(message, privacy: .public)")
        launchLogFile?.append("warning", category, message)
    }

    public func error(_ message: String) {
        osLog.error("\(message, privacy: .public)")
        launchLogFile?.append("error", category, message)
    }
}

/// The file a UI test names in `BAE_UI_TESTING_LOG` when it launches the
/// app, which every logger appends every message to, at every level; nil when
/// the launch names none. The unified log keeps no debug messages and a CI job
/// log shows none of the app's, so the test reads this file back and prints it
/// when it fails. The first message a logger logs opens the file, once for the
/// process; the app stops when it cannot open it, since the test would then
/// read no log.
private let launchLogFile: LogFile? = {
    guard
        let path = ProcessInfo.processInfo.environment["BAE_UI_TESTING_LOG"]
    else { return nil }
    guard let file = LogFile(path: path) else {
        preconditionFailure("cannot open the UI test log at \(path)")
    }
    return file
}()

/// A file that the loggers append every message to.
private final class LogFile: Sendable {
    private let handle: FileHandle
    private let queue = DispatchQueue(label: "fm.bae.log-file")

    /// Opens the file at `path` empty, creating it. Returns nil when it
    /// cannot create the file or open it for writing.
    init?(path: String) {
        guard FileManager.default.createFile(atPath: path, contents: nil),
            let handle = FileHandle(forWritingAtPath: path)
        else { return nil }
        self.handle = handle
    }

    func append(_ level: String, _ category: String, _ message: String) {
        let line =
            "\(Date().timeIntervalSince1970) \(level) [\(category)] \(message)\n"
        queue.async { [handle] in
            handle.write(Data(line.utf8))
        }
    }
}

extension Logger {
    public static func bae(_ category: String) -> BaeLogger {
        BaeLogger(category: category)
    }
}
