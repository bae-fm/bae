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
        installedLogFile.append("debug", category, message)
    }

    public func info(_ message: String) {
        osLog.info("\(message, privacy: .public)")
        installedLogFile.append("info", category, message)
    }

    public func warning(_ message: String) {
        osLog.warning("\(message, privacy: .public)")
        installedLogFile.append("warning", category, message)
    }

    public func error(_ message: String) {
        osLog.error("\(message, privacy: .public)")
        installedLogFile.append("error", category, message)
    }
}

extension BaeLogger {
    /// Append every message every logger logs from now on to `file`, at
    /// every level. The app calls this once, at launch.
    public static func appendEveryMessage(to file: LogFile) {
        installedLogFile.install(file)
    }
}

/// A file that a logger appends every message to, at every level. The
/// unified log keeps no debug messages and a CI job log shows none of the
/// app's, so a UI test names such a file, reads it back and prints it when
/// it fails.
public final class LogFile: Sendable {
    private let handle: FileHandle
    private let queue = DispatchQueue(label: "fm.bae.log-file")

    /// Opens the file at `path` empty, creating it. Returns nil when it
    /// cannot create the file or open it for writing.
    public init?(path: String) {
        guard FileManager.default.createFile(atPath: path, contents: nil),
            let handle = FileHandle(forWritingAtPath: path)
        else { return nil }
        self.handle = handle
    }

    fileprivate func append(
        _ level: String,
        _ category: String,
        _ message: String
    ) {
        let line =
            "\(Date().timeIntervalSince1970) \(level) [\(category)] \(message)\n"
        queue.async { [handle] in
            handle.write(Data(line.utf8))
        }
    }
}

/// The log file `appendEveryMessage` installed, which every logger appends
/// to. There is no log file until the app calls `appendEveryMessage`.
private let installedLogFile = InstalledLogFile()

private final class InstalledLogFile: @unchecked Sendable {
    private let lock = NSLock()
    private var file: LogFile?

    func install(_ file: LogFile) {
        lock.withLock {
            precondition(self.file == nil, "a log file is already installed")
            self.file = file
        }
    }

    func append(_ level: String, _ category: String, _ message: String) {
        lock.withLock { file }?.append(level, category, message)
    }
}

extension Logger {
    public static func bae(_ category: String) -> BaeLogger {
        BaeLogger(category: category)
    }
}
