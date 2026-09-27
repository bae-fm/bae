import BaeKit
import Foundation

@testable import bae

/// One session write, as a recording writer saw it. The recorder lives here
/// because the tests are the only thing that reads a write back — the app
/// writes through the importer and reads the candidate core hands back.
enum CandidateSessionWrite: Equatable, Sendable {
    case paneMove(key: String, move: BridgePaneMove)
    case searchForm(key: String, form: BridgeSearchForm)
    /// A failure no pane states, told to the person.
    case reportedFailure(String)
}

extension CandidateSessionWriter {
    /// Records every write, for a test to read back.
    static func recording(
        _ record: @escaping @Sendable (CandidateSessionWrite) -> Void
    ) -> CandidateSessionWriter {
        CandidateSessionWriter(
            movePane: { key, move in
                record(.paneMove(key: key, move: move))
            },
            setSearchForm: { key, form in
                record(.searchForm(key: key, form: form))
            },
            reportFailure: { error in
                record(.reportedFailure(String(describing: error)))
            }
        )
    }
}

/// Every session write the store made, so a test can read what it would
/// have stored with the candidate.
final class SessionWriteRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var writes: [CandidateSessionWrite] = []

    func record(_ write: CandidateSessionWrite) {
        lock.withLock { writes.append(write) }
    }

    /// The pane moves the store asked core for, for `key`, in order.
    func paneMoves(forKey key: String) -> [BridgePaneMove] {
        lock.withLock {
            writes.compactMap { write in
                if case .paneMove(let written, let move) = write,
                    written == key
                {
                    return move
                }
                return nil
            }
        }
    }

    /// The failures told to the person rather than stated on a pane.
    func reportedFailures() -> [String] {
        lock.withLock {
            writes.compactMap { write in
                if case .reportedFailure(let failure) = write {
                    return failure
                }
                return nil
            }
        }
    }
}
