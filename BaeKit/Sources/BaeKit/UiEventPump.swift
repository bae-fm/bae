import OSLog

private let logger = Logger.bae("UiEventPump")

/// Turns core's fire-and-forget `onEvent` callback into ordered, main-actor
/// delivery. Each event core emits is buffered on an unbounded stream and
/// drained one at a time on the main actor, so the sink sees events in the order
/// core produced them. Once the pump is torn down the stream finishes and its
/// drain task ends; any event core delivers afterward finds a terminated stream
/// and is dropped with a warning rather than delivered to the sink.
public final class UiEventPump: UiEventCallback, @unchecked Sendable {
    private let eventContinuation: AsyncStream<BridgeUiEvent>.Continuation
    private let eventDeliveryTask: Task<Void, Never>

    public init(sink: @escaping @MainActor @Sendable (BridgeUiEvent) -> Void) {
        let eventStream = AsyncStream.makeStream(
            of: BridgeUiEvent.self,
            bufferingPolicy: .unbounded
        )
        eventContinuation = eventStream.continuation
        eventDeliveryTask = Task { @MainActor in
            for await event in eventStream.stream {
                sink(event)
            }
        }
    }

    public func onEvent(event: BridgeUiEvent) {
        switch eventContinuation.yield(event) {
        case .enqueued:
            break

        case .dropped(let event):
            logger.warning(
                "Dropped UI event because the delivery stream buffer dropped it: \(String(describing: event))"
            )

        case .terminated:
            logger.warning(
                "Dropped UI event because the delivery stream has terminated: \(String(describing: event))"
            )

        @unknown default:
            logger.warning(
                "Dropped UI event because the delivery stream returned an unknown result: \(String(describing: event))"
            )
        }
    }

    deinit {
        eventContinuation.finish()
        eventDeliveryTask.cancel()
    }
}
