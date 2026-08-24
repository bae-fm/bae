import OSLog

private let logger = Logger.bae("UiEventDispatcher")

/// Dispatches a `BridgeUiEvent` off `UiEventPump` into the shared stores and
/// services every Apple platform writes: `PlaybackStore`, `MediaControlService`,
/// and `AppService`. Most variants apply identically on every
/// platform — `dispatch` handles exactly those and reports whether it did.
/// On macOS, import-only variants resolve to `.unhandled`; the platform sink's
/// `onUnhandled` owns what happens to them.
public enum UiEventDispatcher {
    /// Whether a shared arm consumed the event. The platform sink decides what
    /// to do with `.unhandled` events: macOS handles its desktop-only events
    /// and drops the rest; iOS ignores the known desktop-only events and traps
    /// on anything else.
    public enum Outcome {
        case handled
        case unhandled
    }

    /// One exhaustive switch over every `BridgeUiEvent` variant, with no
    /// `default` — a new variant fails the BaeKit build until it's routed here
    /// explicitly. Groups variants by concern and routes each group to a
    /// helper, so no single switch grows past the lint complexity threshold.
    @MainActor
    public static func dispatch(
        _ event: BridgeUiEvent,
        appService: AppService
    ) -> Outcome {
        switch event {
        case .queueItemsAdded(let count):
            appService.applyQueueItemsAdded(count)

        case .playbackError(let reason):
            appService.showError(reason)

        case .error(let error):
            appService.showError(error)

        #if os(macOS)
            case .candidateImportLoudnessProgress,
                .candidateSignalsUpdated,
                .importQueueIdentifyProgress,
                .watchedFolderScanFailed:
                return .unhandled
        #endif
        }
        return .handled
    }

    /// Builds the sink a `UiEventPump` drains on the main actor: weak
    /// `appService` capture with drop-with-warning after library teardown,
    /// shared `dispatch`, then the platform's `onUnhandled` for events no
    /// shared arm consumes. Generic over the platform subclass so
    /// `onUnhandled` receives the concrete type without a downcast.
    @MainActor
    public static func makeSink<Service: AppService>(
        appService: Service,
        onUnhandled:
            @escaping @MainActor @Sendable (BridgeUiEvent, Service) ->
            Void
    ) -> @MainActor @Sendable (BridgeUiEvent) -> Void {
        { [weak appService] event in
            guard let appService else {
                logger.warning(
                    "Dropped UI event because its target was deallocated: \(String(describing: event))"
                )
                return
            }
            if dispatch(event, appService: appService) == .unhandled {
                onUnhandled(event, appService)
            }
        }
    }
}
