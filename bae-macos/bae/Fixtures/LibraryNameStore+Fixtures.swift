import BaeKit
import Foundation
import SwiftUI

@testable import bae

/// A name that answers once and then waits until cancelled.
private final class FixedLibraryName: PulledSubscription,
    @unchecked Sendable
{
    private let lock = NSLock()
    private var answered = false
    private let name: String
    private var waiter: CheckedContinuation<String, any Error>?

    init(name: String) { self.name = name }

    func next() async throws -> String {
        let first = lock.withLock {
            defer { answered = true }
            return !answered
        }
        if first { return name }
        return try await withCheckedThrowingContinuation { continuation in
            lock.withLock { waiter = continuation }
        }
    }

    func cancel() async throws {
        let waiter = lock.withLock {
            defer { self.waiter = nil }
            return self.waiter
        }
        waiter?.resume(throwing: BridgeError.Cancelled)
    }
}

extension LibraryNameStore {
    /// A store whose name is `name` once a screen follows it, for
    /// shots.
    static func stub(name: String) -> LibraryNameStore {
        LibraryNameStore(
            subscribe: { PulledValues(FixedLibraryName(name: name)) },
            current: { name },
            showError: { _ in }
        )
    }
}
