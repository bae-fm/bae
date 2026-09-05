import Foundation

@testable import bae

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
