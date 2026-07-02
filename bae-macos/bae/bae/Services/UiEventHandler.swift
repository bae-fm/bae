import Foundation

/// Receives UI events from core via the unified event stream and
/// dispatches them to the reducer on the main actor.
final class UiEventHandler: UiEventCallback, @unchecked Sendable {
    private let playbackStore: PlaybackStore
    private let configStore: ConfigStore
    private let importStore: ImportStore
    private let libraryStore: LibraryStore
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
        self.appService = appService
        self.uiStore = uiStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.exportStore = exportStore
    }

    func onEvent(event: BridgeUiEvent) {
        // The library was torn down (closed/switched) while an event was in
        // flight; the weak `appService` is gone, so there's nothing to update.
        guard let appService else { return }
        let playbackStore = playbackStore
        let configStore = configStore
        let importStore = importStore
        let libraryStore = libraryStore
        let uiStore = uiStore
        let outboxStore = outboxStore
        let downloadStore = downloadStore
        let exportStore = exportStore
        Task { @MainActor in
            let context = ReducerContext(
                playbackStore: playbackStore,
                configStore: configStore,
                importStore: importStore,
                libraryStore: libraryStore,
                appService: appService,
                uiStore: uiStore,
                outboxStore: outboxStore,
                downloadStore: downloadStore,
                exportStore: exportStore
            )
            UiEventReducer.reduce(event, into: context)
        }
    }
}
