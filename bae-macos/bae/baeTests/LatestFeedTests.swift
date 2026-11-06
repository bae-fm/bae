import BaeKit
import Foundation
import Observation
import Testing

/// A stream a test feeds in place of core: it holds what the test sets, and
/// a pull answers with the key changes or the failed reads the test queues.
private final class QueuedKeyStream: KeyedValueStream, @unchecked Sendable {
    private let lock = NSLock()
    private var held: [Data: BridgeFastValue] = [:]
    private var queued: [Result<[Data], any Error>] = []
    private var puller: CheckedContinuation<[Data], any Error>?

    init() {
        super.init(noHandle: KeyedValueStream.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a queued stream is made by init()")
    }

    /// Hold `value` under `key`, and queue that the key started.
    func start(_ key: Data, holding value: BridgeFastValue) {
        lock.recording { held[key] = value }
        answer(.success([key]))
    }

    /// Queue a failed read of the feed.
    func fail(_ error: any Error) {
        answer(.failure(error))
    }

    override func latest(key: Data) -> BridgeFastValue? {
        lock.withLock { held[key] }
    }

    override func next() async throws -> [Data] {
        try await withCheckedThrowingContinuation { continuation in
            let ready: Result<[Data], any Error>? = lock.withLock {
                if queued.isEmpty {
                    puller = continuation
                    return nil
                }
                return queued.removeFirst()
            }
            if let ready { continuation.resume(with: ready) }
        }
    }

    override func cancel() {
        let puller = lock.withLock {
            held = [:]
            let puller = self.puller
            self.puller = nil
            return puller
        }
        puller?.resume(throwing: BridgeError.Cancelled)
    }

    private func answer(_ read: Result<[Data], any Error>) {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            if puller == nil { queued.append(read) }
            return puller
        }
        puller?.resume(with: read)
    }
}

/// A handle-less `AppHandle` whose upload total is `stream`.
private final class UploadTotalHandle: AppHandle, @unchecked Sendable {
    let stream = QueuedKeyStream()

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("an upload-total handle is made by init()")
    }

    override func feedUploadTotalTransfer() -> KeyedValueStream {
        stream
    }
}

/// What a fed stream's failed reads handed on.
@MainActor
@Observable
private final class FailedReads {
    var count = 0
}

@MainActor
@Suite("LatestFeed")
struct LatestFeedTests {
    @Test(
        "a pulled key change reaches the key's watcher, and a failed read reaches onError"
    )
    func pulledChangesAndFailuresAreHandedOn() async throws {
        let handle = UploadTotalHandle()
        let failed = FailedReads()
        let stream = LatestStream<BridgeUploadTransfer>.uploadTotal()
        let feed = stream.feed(from: handle) { _ in failed.count += 1 }
        let followed = FollowedValue(stream.only)
        #expect(followed.value == nil)

        let sending = BridgeUploadTransfer(bar: nil, throughputBps: 5)
        handle.stream.start(Data(), holding: .uploadTotal(transfer: sending))
        try await Wait.until { followed.value == sending }

        handle.stream.fail(
            BridgeError.Diagnostic(category: .database, detail: "read failed")
        )
        try await Wait.until { failed.count == 1 }

        followed.stop()
        feed.cancel()
    }
}
