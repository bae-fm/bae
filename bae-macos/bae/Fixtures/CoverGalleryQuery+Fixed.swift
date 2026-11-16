import BaeKit
import Foundation

@testable import bae

/// A picker's live read that answers each request with `gallery`.
extension CoverGalleryQuery {
    /// `delivered`, when given, records the focus each value answers.
    static func fixed(
        _ gallery: BridgeCoverGallery,
        delivered: DeliveredFocuses? = nil
    ) -> CoverGalleryQuery {
        let (stream, continuation) = AsyncStream<BridgeCoverGallerySnapshot>
            .makeStream()
        let pulls = FixedPulls(stream.makeAsyncIterator())
        return CoverGalleryQuery(
            setRequest: { owner, filter, focus in
                continuation.yield(
                    BridgeCoverGallerySnapshot(
                        request: BridgeCoverGalleryRequest(
                            owner: owner,
                            filter: filter,
                            focus: focus
                        ),
                        gallery: gallery.focused(on: focus)
                    )
                )
            },
            values: PulledValues(
                read: {
                    let snapshot = try await pulls.next()
                    delivered?.record(snapshot.request.focus)
                    return snapshot
                },
                cancel: { continuation.finish() }
            )
        )
    }
}

/// The focus each value of a fixed query answered, in the order the query
/// delivered them.
final class DeliveredFocuses: @unchecked Sendable {
    private let lock = NSLock()
    private var focuses: [BridgeCoverKey?] = []

    func record(_ focus: BridgeCoverKey?) {
        lock.withLock { focuses.append(focus) }
        Wait.happened()
    }

    /// How many values the query has delivered.
    var count: Int {
        lock.withLock { focuses.count }
    }

    /// Whether the query has delivered a value that focuses `key`.
    func delivered(_ key: BridgeCoverKey) -> Bool {
        lock.withLock { focuses.contains(key) }
    }
}

/// The one reader of a stream, which waits for each request's value in turn.
private final class FixedPulls: @unchecked Sendable {
    private var iterator: AsyncStream<BridgeCoverGallerySnapshot>.AsyncIterator

    init(_ iterator: AsyncStream<BridgeCoverGallerySnapshot>.AsyncIterator) {
        self.iterator = iterator
    }

    func next() async throws -> BridgeCoverGallerySnapshot {
        guard let snapshot = await iterator.next() else {
            throw CancellationError()
        }
        return snapshot
    }
}
