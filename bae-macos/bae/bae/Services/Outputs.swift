import BaeKit
import Foundation

/// Controls for the export queue: enqueue a release export
/// out to a folder, pause/resume the queue, and cancel a release's export.
/// Mirrors `Downloads`, wrapping the corresponding `handle.*` methods. The queue itself lives in bae-library; the Storage Manager's
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
    /// A preset for a new format as core makes it, under an id of its own.
    let newSavePreset:
        @Sendable (_ existing: [BridgeSavePreset]) -> BridgeSavePreset
    /// Replace configured export presets.
    let setSavePresets:
        @Sendable (_ presets: [BridgeSavePreset]) async throws -> Void
    /// Set the one file name pattern every save names its files by.
    let setFilenamePieces:
        @Sendable (_ pieces: [BridgeSaveFilenamePiece]) async throws -> Void
    /// Set whether each saved file embeds the release's cover.
    let setSaveEmbedCover: @Sendable (_ on: Bool) async throws -> Void
    /// Set whether a release save writes the cover as cover.jpg in its folder.
    let setSaveCoverFile: @Sendable (_ on: Bool) async throws -> Void
    /// Set whether each saved file carries ReplayGain tags.
    let setSaveLoudnessTags: @Sendable (_ on: Bool) async throws -> Void
    /// Set the folder name pattern a release save names its folder by.
    let setFolderPieces:
        @Sendable (_ pieces: [BridgeSaveFolderPiece]) async throws -> Void

    init(
        newSavePreset:
            @escaping @Sendable ([BridgeSavePreset]) -> BridgeSavePreset,
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
        setFilenamePieces:
            @escaping @Sendable ([BridgeSaveFilenamePiece]) async throws
            -> Void = { _ in
            },
        setFolderPieces:
            @escaping @Sendable ([BridgeSaveFolderPiece]) async throws
            -> Void = { _ in
            },
        setSaveEmbedCover: @escaping @Sendable (Bool) async throws -> Void = {
            _ in
        },
        setSaveCoverFile: @escaping @Sendable (Bool) async throws -> Void = {
            _ in
        },
        setSaveLoudnessTags:
            @escaping @Sendable (Bool) async throws -> Void = { _ in
            }
    ) {
        self.newSavePreset = newSavePreset
        self.enqueueExport = enqueueExport
        self.enqueueReleaseSave = enqueueReleaseSave
        self.setOutputsPaused = setOutputsPaused
        self.cancelOutput = cancelOutput
        self.setSavePresets = setSavePresets
        self.setFilenamePieces = setFilenamePieces
        self.setFolderPieces = setFolderPieces
        self.setSaveEmbedCover = setSaveEmbedCover
        self.setSaveCoverFile = setSaveCoverFile
        self.setSaveLoudnessTags = setSaveLoudnessTags
    }

    convenience init(handle: any AppHandleProtocol, host: BridgeHost) {
        self.init(
            newSavePreset: { host.newSavePreset(existing: $0) },
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
            setFilenamePieces: {
                try await handle.setFilenamePieces(pieces: $0)
            },
            setFolderPieces: {
                try await handle.setFolderPieces(pieces: $0)
            },
            setSaveEmbedCover: { try await handle.setSaveEmbedCover(on: $0) },
            setSaveCoverFile: { try await handle.setSaveCoverFile(on: $0) },
            setSaveLoudnessTags: {
                try await handle.setSaveLoudnessTags(on: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        static func stub() -> Outputs {
            Outputs(newSavePreset: { _ in PreviewData.savePresets[0] })
        }
    #endif
}
