import BaeKit
import Foundation

@testable import bae

/// One session write, as a recording writer saw it. The recorder lives here
/// because the tests are the only thing that reads a write back — the app
/// writes through the importer and reads the candidate core hands back.
enum CandidateSessionWrite: Equatable, Sendable {
    case presentation(key: String, presentation: BridgeMetadataPresentation)
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
            setPresentation: { key, presentation in
                record(.presentation(key: key, presentation: presentation))
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

    /// The surfaces put in the metadata slot for `key`, in order.
    func presentations(forKey key: String) -> [BridgeMetadataPresentation] {
        lock.withLock {
            writes.compactMap { write in
                if case .presentation(let written, let presentation) = write,
                    written == key
                {
                    return presentation
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
