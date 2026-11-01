import OSLog

private let logger = Logger.bae("UiEventDispatcher")

/// Dispatches a `BridgeUiEvent` off `UiEventPump` into the shared stores and
/// services every Apple platform writes. Every variant applies identically
/// on every platform.
public enum UiEventDispatcher {
    /// One exhaustive switch over every `BridgeUiEvent` variant, with no
    /// `default` — a new variant fails the BaeKit build until it's routed here
    /// explicitly.
    @MainActor
    public static func dispatch(
        _ event: BridgeUiEvent,
        appService: AppService
    ) {
        switch event {
        case .queueItemsAdded(let count):
            appService.applyQueueItemsAdded(count)
        }
    }

    /// Builds the sink a `UiEventPump` drains on the main actor: weak
    /// `appService` capture with drop-with-warning after library teardown,
    /// then `dispatch`.
    @MainActor
    public static func makeSink(
        appService: AppService
    ) -> @MainActor @Sendable (BridgeUiEvent) -> Void {
        { [weak appService] event in
            guard let appService else {
                logger.warning(
                    "Dropped UI event because its target was deallocated: \(String(describing: event))"
                )
                return
            }
            dispatch(event, appService: appService)
        }
    }
}
