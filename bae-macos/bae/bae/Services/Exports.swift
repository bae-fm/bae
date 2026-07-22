import BaeKit
import Foundation

/// Controls for the in-memory export queue: enqueue a release export
/// out to a folder, pause/resume the queue, cancel a release's export, and retry
/// failed ones. Mirrors `Downloads`, wrapping the corresponding `handle.*`
/// methods. The queue itself lives in bae-core; the Exporting pane reads its
/// state from `ExportStore` (the export projection is the sole writer).
final class Exports: Sendable, Observable {
    /// Enqueue a verbatim release export to `targetDir`. It joins the serial
    /// output queue; the worker drains it one release at a time. Fire-and-forget
    /// — progress and queue state arrive via `exportQueueChanged` events.
    let enqueueExport:
        @Sendable (_ releaseId: String, _ targetDir: String) async throws ->
            Void
    /// Enqueue a release-level save to `targetDir` under `presetId`. The preset
    /// is captured whole at enqueue, so a later config edit can't drift it.
    let enqueueReleaseSave:
        @Sendable (
            _ releaseId: String, _ targetDir: String, _ presetId: String
        ) async throws -> Void
    /// Pause or resume the export queue. The in-flight export finishes; the
    /// queue stops starting new ones until resumed.
    let setExportsPaused: @Sendable (_ paused: Bool) -> Void
    /// Cancel a release's export — drops a queued/failed entry or aborts the
    /// in-flight one (a partial copy never lands its destination file).
    let cancelExport: @Sendable (_ releaseId: String) -> Void
    /// Retry every failed export now (flips them back to queued).
    let retryExports: @Sendable () -> Void
    /// Replace configured export presets.
    let setExportPresets:
        @Sendable (_ presets: [BridgeExportPreset]) throws -> Void
    let setDefaultTrackSavePreset: @Sendable (_ presetId: String) throws -> Void
    let setDefaultReleaseSavePreset:
        @Sendable (_ presetId: String) throws -> Void

    init(
        enqueueExport:
            @escaping @Sendable (String, String)
            async throws -> Void =
            { _, _ in
            },
        enqueueReleaseSave:
            @escaping @Sendable (String, String, String)
            async throws -> Void =
            { _, _, _ in
            },
        setExportsPaused: @escaping @Sendable (Bool) -> Void = { _ in },
        cancelExport: @escaping @Sendable (String) -> Void = { _ in },
        retryExports: @escaping @Sendable () -> Void = {},
        setExportPresets:
            @escaping @Sendable ([BridgeExportPreset]) throws -> Void = { _ in
            },
        setDefaultTrackSavePreset:
            @escaping @Sendable (String) throws -> Void = { _ in
            },
        setDefaultReleaseSavePreset:
            @escaping @Sendable (String) throws -> Void = { _ in
            }
    ) {
        self.enqueueExport = enqueueExport
        self.enqueueReleaseSave = enqueueReleaseSave
        self.setExportsPaused = setExportsPaused
        self.cancelExport = cancelExport
        self.retryExports = retryExports
        self.setExportPresets = setExportPresets
        self.setDefaultTrackSavePreset = setDefaultTrackSavePreset
        self.setDefaultReleaseSavePreset = setDefaultReleaseSavePreset
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            enqueueExport: {
                try await handle.enqueueExport(releaseId: $0, targetDir: $1)
            },
            enqueueReleaseSave: {
                try await handle.enqueueReleaseSave(
                    releaseId: $0,
                    targetDir: $1,
                    presetId: $2
                )
            },
            setExportsPaused: { handle.setExportsPaused(paused: $0) },
            cancelExport: { handle.cancelExport(releaseId: $0) },
            retryExports: { handle.retryExports() },
            setExportPresets: { try handle.setExportPresets(presets: $0) },
            setDefaultTrackSavePreset: {
                try handle.setDefaultTrackSavePreset(presetId: $0)
            },
            setDefaultReleaseSavePreset: {
                try handle.setDefaultReleaseSavePreset(presetId: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        static let stub = Exports()
    #endif
}
