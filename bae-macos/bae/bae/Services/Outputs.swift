import BaeKit
import Foundation

/// Controls for the export queue: enqueue a release export
/// out to a folder, pause/resume the queue, and cancel a release's export.
/// Mirrors `Downloads`, wrapping the corresponding `handle.*` methods. The queue itself lives in bae-core; the Storage Manager's
/// inspector reads a release's entry in it.
final class Outputs: Sendable, Observable {
    /// Enqueue a verbatim release export to `targetDir`. It joins the serial
    /// output queue; the worker drains it one release at a time. The queue
    /// arrives through its live read, and each export's percent through its
    /// transfers.
    let enqueueExport:
        @Sendable (_ releaseId: String, _ targetDir: URL) async throws ->
            Void
    /// Enqueue a release-level save to `targetDir` under `presetId`. The preset
    /// is captured whole at enqueue, so a later config edit can't drift it.
    let enqueueReleaseSave:
        @Sendable (
            _ releaseId: String, _ targetDir: URL, _ presetId: String
        ) async throws -> Void
    /// Pause or resume the output queue. In-flight work finishes; the queue
    /// stops starting new operations until resumed.
    let setOutputsPaused: @Sendable (_ paused: Bool) async throws -> Void
    /// Cancel a release's export — drops a queued/failed entry or aborts the
    /// in-flight one (a partial copy never lands its destination file).
    let cancelOutput: @Sendable (_ releaseId: String) async throws -> Void
    /// Replace configured export presets.
    let setSavePresets:
        @Sendable (_ presets: [BridgeSavePreset]) async throws -> Void
    let setDefaultTrackSavePreset:
        @Sendable (_ presetId: String) async throws -> Void
    let setDefaultReleaseSavePreset:
        @Sendable (_ presetId: String) async throws -> Void

    init(
        enqueueExport:
            @escaping @Sendable (String, URL)
            async throws -> Void =
            { _, _ in
            },
        enqueueReleaseSave:
            @escaping @Sendable (String, URL, String)
            async throws -> Void =
            { _, _, _ in
            },
        setOutputsPaused: @escaping @Sendable (Bool) async throws -> Void = {
            _ in
        },
        cancelOutput: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        setSavePresets:
            @escaping @Sendable ([BridgeSavePreset]) async throws -> Void = {
                _ in
            },
        setDefaultTrackSavePreset:
            @escaping @Sendable (String) async throws -> Void = { _ in
            },
        setDefaultReleaseSavePreset:
            @escaping @Sendable (String) async throws -> Void = { _ in
            }
    ) {
        self.enqueueExport = enqueueExport
        self.enqueueReleaseSave = enqueueReleaseSave
        self.setOutputsPaused = setOutputsPaused
        self.cancelOutput = cancelOutput
        self.setSavePresets = setSavePresets
        self.setDefaultTrackSavePreset = setDefaultTrackSavePreset
        self.setDefaultReleaseSavePreset = setDefaultReleaseSavePreset
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            enqueueExport: {
                try await handle.enqueueExport(
                    releaseId: $0,
                    targetDir: $1.pathBytes
                )
            },
            enqueueReleaseSave: {
                try await handle.enqueueReleaseSave(
                    releaseId: $0,
                    targetDir: $1.pathBytes,
                    presetId: $2
                )
            },
            setOutputsPaused: { try await handle.setOutputsPaused(paused: $0) },
            cancelOutput: { try await handle.cancelOutput(releaseId: $0) },
            setSavePresets: { try await handle.setSavePresets(presets: $0) },
            setDefaultTrackSavePreset: {
                try await handle.setDefaultTrackSavePreset(presetId: $0)
            },
            setDefaultReleaseSavePreset: {
                try await handle.setDefaultReleaseSavePreset(presetId: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        static func stub() -> Outputs {
            Outputs()
        }
    #endif
}
