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
        UITestLogFile.append("debug", category, message)
    }

    public func info(_ message: String) {
        osLog.info("\(message, privacy: .public)")
        UITestLogFile.append("info", category, message)
    }

    public func warning(_ message: String) {
        osLog.warning("\(message, privacy: .public)")
        UITestLogFile.append("warning", category, message)
    }

    public func error(_ message: String) {
        osLog.error("\(message, privacy: .public)")
        UITestLogFile.append("error", category, message)
    }
}

/// The file a UI test names in `BAE_UI_TESTING_LOG`, which every message the
/// app logs is also appended to, at every level. The unified log keeps no
/// debug messages and a CI job log shows none of the app's, so the test reads
/// this file back and prints it when it fails.
private enum UITestLogFile {
    private static let queue = DispatchQueue(label: "fm.bae.ui-test-log")
    private static let handle: FileHandle? = {
        guard
            let path = ProcessInfo.processInfo
                .environment["BAE_UI_TESTING_LOG"]
        else { return nil }
        FileManager.default.createFile(atPath: path, contents: nil)
        return FileHandle(forWritingAtPath: path)
    }()

    static func append(_ level: String, _ category: String, _ message: String) {
        guard let handle else { return }
        let line =
            "\(Date().timeIntervalSince1970) \(level) [\(category)] \(message)\n"
        queue.async {
            handle.write(Data(line.utf8))
        }
    }
}

extension Logger {
    public static func bae(_ category: String) -> BaeLogger {
        BaeLogger(category: category)
    }
}
