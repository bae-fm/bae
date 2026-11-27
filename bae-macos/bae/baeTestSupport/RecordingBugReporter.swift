import BaeKit
import Synchronization

/// A bug reporter a test reads: it keeps the operation of each bug reported
/// to it, in the order they came, from whatever thread found it.
final class RecordingBugReporter: BridgeBugReporter {
    private let reported = Mutex<[String]>([])

    /// The operation of each bug reported so far.
    var operations: [String] { reported.withLock { $0 } }

    func report(operation: String, detail _: String) {
        reported.withLock { $0.append(operation) }
    }
}

extension BugReports {
    /// The reporter a test gives the views it hosts, when it reads none of
    /// what they report.
    static func recording() -> BugReports {
        BugReports(RecordingBugReporter())
    }
}
