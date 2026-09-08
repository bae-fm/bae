import BaeKit
import Foundation

@testable import bae

/// One session write, as a recording writer saw it. The recorder lives here
/// because the tests are the only thing that reads a write back — the app
/// writes through the importer and reads the candidate core hands back.
enum CandidateSessionWrite: Equatable, Sendable {
    case presentation(key: String, presentation: BridgeMetadataPresentation)
    case searchForm(key: String, form: BridgeSearchForm)
    case error(key: String, error: String?)
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
            setError: { key, error in record(.error(key: key, error: error)) },
            reportFailure: { _ in }
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

    /// The banner lines written for `key`, in order; `nil` is a clear.
    func errors(forKey key: String) -> [String?] {
        lock.withLock {
            writes.compactMap { write in
                if case .error(let written, let error) = write, written == key {
                    return .some(error)
                }
                return nil
            }
        }
    }
}
