import Foundation
import os.log

private let logger = Logger.bae("UiEventHandler")

/// Receives UI events from core via the unified event stream and
/// dispatches them to the reducer on the main actor.
final class UiEventHandler: UiEventCallback, @unchecked Sendable {
    private let eventContinuation: AsyncStream<BridgeUiEvent>.Continuation
    private let eventDeliveryTask: Task<Void, Never>

    init(
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        importStore: ImportStore,
        libraryStore: LibraryStore,
        projectionRegistry: ProjectionRegistry,
        appService: AppService,
        uiStore: UiStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        exportStore: ExportStore
    ) {
        let eventStream = AsyncStream.makeStream(
            of: BridgeUiEvent.self,
            bufferingPolicy: .unbounded
        )
        let deliveryTarget = UiEventDeliveryTarget(
            playbackStore: playbackStore,
            configStore: configStore,
            importStore: importStore,
            libraryStore: libraryStore,
            projectionRegistry: projectionRegistry,
            appService: appService,
            uiStore: uiStore,
            outboxStore: outboxStore,
            downloadStore: downloadStore,
            exportStore: exportStore
        )
        self.eventContinuation = eventStream.continuation
        self.eventDeliveryTask = Task { @MainActor in
            for await event in eventStream.stream {
                deliveryTarget.reduce(event)
            }
        }
    }

    func onEvent(event: BridgeUiEvent) {
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

private final class UiEventDeliveryTarget: @unchecked Sendable {
    private let playbackStore: PlaybackStore
    private let configStore: ConfigStore
    private let importStore: ImportStore
    private let libraryStore: LibraryStore
    private let projectionRegistry: ProjectionRegistry
    /// Weak so the handler doesn't retain the `AppService` it dispatches to.
    /// The handler is owned by the Rust subscribe task (held alive by the
    /// `AppHandle`'s runtime), and `AppService` owns the `AppHandle`; a strong
    /// reference here would close that cycle and keep the whole library — handle,
    /// runtime, stores — alive forever. With it weak, dropping `AppService`
    /// (closing or switching libraries) releases the handle, drops the event
    /// bus's sender, ends the subscribe task, and tears down the library.
    private weak var appService: AppService?
    private let uiStore: UiStore
    private let outboxStore: OutboxStore
    private let downloadStore: DownloadStore
    private let exportStore: ExportStore

    init(
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        importStore: ImportStore,
        libraryStore: LibraryStore,
        projectionRegistry: ProjectionRegistry,
        appService: AppService,
        uiStore: UiStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        exportStore: ExportStore
    ) {
        self.playbackStore = playbackStore
        self.configStore = configStore
        self.importStore = importStore
        self.libraryStore = libraryStore
        self.projectionRegistry = projectionRegistry
        self.appService = appService
        self.uiStore = uiStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.exportStore = exportStore
    }

    @MainActor
    func reduce(_ event: BridgeUiEvent) {
        // The library was torn down (closed/switched) while events were queued;
        // the weak `appService` is gone, so this handler has no live target.
        guard let appService else {
            logger.warning(
                "Dropped UI event because the library event target was deallocated: \(String(describing: event))"
            )
            return
        }
        let context = ReducerContext(
            playbackStore: playbackStore,
            configStore: configStore,
            importStore: importStore,
            libraryStore: libraryStore,
            projectionRegistry: projectionRegistry,
            appService: appService,
            uiStore: uiStore,
            outboxStore: outboxStore,
            downloadStore: downloadStore,
            exportStore: exportStore
        )
        UiEventReducer.reduce(event, into: context)
    }
}
