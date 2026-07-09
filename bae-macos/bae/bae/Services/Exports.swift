import BaeKit
import Foundation

/// Controls for the in-memory export queue: enqueue a release export
/// out to a folder, pause/resume the queue, cancel a release's export, and retry
/// failed ones. Mirrors `Downloads`, wrapping the corresponding `handle.*`
/// methods. The queue itself lives in bae-core; the Exporting pane reads its
/// state from `ExportStore` (the export projection is the sole writer).
final class Exports: Sendable, Observable {
    /// Enqueue a release export to `targetDir`. It joins
    /// the serial export queue; the worker drains it one release at a time.
    /// Fire-and-forget — progress and queue state arrive via `exportQueueChanged`
    /// events.
    let enqueueExport:
        @Sendable (
            _ releaseId: String, _ targetDir: String,
            _ selection: BridgeExportSelection
        ) async throws -> Void
    /// Pause or resume the export queue. The in-flight export finishes; the
    /// queue stops starting new ones until resumed.
    let setExportsPaused: @Sendable (_ paused: Bool) -> Void
    /// Cancel a release's export — drops a queued/failed entry or aborts the
    /// in-flight one (a partial copy never lands its destination file).
    let cancelExport: @Sendable (_ releaseId: String) -> Void
    /// Retry every failed export now (flips them back to queued).
    let retryExports: @Sendable () -> Void
    /// Set where release exports write: prompt each time, or a fixed folder.
    /// The Preferences control drives this; the change round-trips back through
    /// a `configChanged` event into `ConfigStore`.
    let setExportLocation:
        @Sendable (_ location: BridgeExportLocation) throws -> Void
    /// Set the template for a single-track export's suggested filename. The
    /// change round-trips back through a `configChanged` event.
    let setExportFilenameTemplate: @Sendable (_ template: String) throws -> Void
    /// Replace configured export presets.
    let setExportPresets:
        @Sendable (_ presets: [BridgeExportPreset]) throws -> Void
    let setDefaultTrackExportSelection:
        @Sendable (_ selection: BridgeExportSelection) throws -> Void
    let setDefaultReleaseExportSelection:
        @Sendable (_ selection: BridgeExportSelection) throws -> Void

    init(
        enqueueExport:
            @escaping @Sendable (String, String, BridgeExportSelection)
            async throws -> Void =
            { _, _, _ in
            },
        setExportsPaused: @escaping @Sendable (Bool) -> Void = { _ in },
        cancelExport: @escaping @Sendable (String) -> Void = { _ in },
        retryExports: @escaping @Sendable () -> Void = {},
        setExportLocation:
            @escaping @Sendable (BridgeExportLocation) throws -> Void = { _ in
            },
        setExportFilenameTemplate:
            @escaping @Sendable (String) throws -> Void = { _ in },
        setExportPresets:
            @escaping @Sendable ([BridgeExportPreset]) throws -> Void = { _ in
            },
        setDefaultTrackExportSelection:
            @escaping @Sendable (BridgeExportSelection) throws -> Void = { _ in
            },
        setDefaultReleaseExportSelection:
            @escaping @Sendable (BridgeExportSelection) throws -> Void = { _ in
            }
    ) {
        self.enqueueExport = enqueueExport
        self.setExportsPaused = setExportsPaused
        self.cancelExport = cancelExport
        self.retryExports = retryExports
        self.setExportLocation = setExportLocation
        self.setExportFilenameTemplate = setExportFilenameTemplate
        self.setExportPresets = setExportPresets
        self.setDefaultTrackExportSelection = setDefaultTrackExportSelection
        self.setDefaultReleaseExportSelection =
            setDefaultReleaseExportSelection
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            enqueueExport: {
                try await handle.enqueueExport(
                    releaseId: $0,
                    targetDir: $1,
                    selection: $2
                )
            },
            setExportsPaused: { handle.setExportsPaused(paused: $0) },
            cancelExport: { handle.cancelExport(releaseId: $0) },
            retryExports: { handle.retryExports() },
            setExportLocation: { try handle.setExportLocation(location: $0) },
            setExportFilenameTemplate: {
                try handle.setExportFilenameTemplate(template: $0)
            },
            setExportPresets: { try handle.setExportPresets(presets: $0) },
            setDefaultTrackExportSelection: {
                try handle.setDefaultTrackExportSelection(selection: $0)
            },
            setDefaultReleaseExportSelection: {
                try handle.setDefaultReleaseExportSelection(selection: $0)
            }
        )
    }

    // periphery:ignore
    static let stub = Exports()
}
