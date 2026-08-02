import BaeKit
import Foundation

/// The services the mapping pane's actions drive.
struct ImportMappingServices {
    let importer: Importer
    /// Re-reading the folder against the release picked for it runs through
    /// the prefetch, which is what produces the mapping table.
    let library: Library
    let importStore: ImportStore
    let previewAudio: PreviewAudio
    /// Show a document (a log, a text file, a track sheet) in the viewer: its
    /// name, then its path on disk.
    let openDocument: (String, String) -> Void
    /// Show the folder's images in the lightbox, at this file's path.
    let openImage: (String) -> Void
    /// Surface a failed call to the user. Prose the caller already localized.
    let onError: (String) -> Void
}

/// What the mapping table's controls do to a candidate.
///
/// Separate from the views so the pane's behaviour — what excluding a file
/// leaves behind, what naming a row changes, what assigning a cue to a disc
/// re-reads — is exercised without a view hierarchy.
enum ImportMappingFlow {
    @MainActor
    static func actions(
        key: String,
        services: ImportMappingServices
    ) -> ImportMappingActions {
        ImportMappingActions(
            setRole: roleAction(key: key, services: services),
            bindSheet: bindingAction(key: key, services: services),
            setSheetDisc: discAction(key: key, services: services),
            openDocument: services.openDocument,
            openImage: services.openImage,
            preview: { path in services.previewAudio.previewPlay(path) },
            stopPreview: { services.previewAudio.previewStop() },
            editTrack: { track in
                editTrack(
                    key: key,
                    track: track,
                    importStore: services.importStore
                )
            },
            chooseFile: { trackId, audio in
                chooseFile(
                    key: key,
                    trackId: trackId,
                    audio: audio,
                    importStore: services.importStore
                )
            },
            drop: { trackId in
                drop(
                    key: key,
                    trackId: trackId,
                    importStore: services.importStore
                )
            },
            exclude: { fileId in
                start {
                    await exclude(key: key, fileId: fileId, services: services)
                }
            },
        )
    }

    /// Begin one of the table's decisions. The user asked for it and nothing in
    /// the pane takes it back, so the work runs to completion on its own; the
    /// reads it triggers are held on the candidate, where a second pick cancels
    /// the first.
    @MainActor
    private static func start(
        _ work: @escaping @MainActor () async -> Void
    ) {
        Task { @MainActor in await work() }
    }

    @MainActor
    private static func roleAction(
        key: String,
        services: ImportMappingServices
    ) -> (String, BridgeFileRoleChoice) -> Void {
        { fileId, choice in
            start {
                await setRole(
                    key: key,
                    fileId: fileId,
                    choice: choice,
                    services: services
                )
            }
        }
    }

    @MainActor
    private static func bindingAction(
        key: String,
        services: ImportMappingServices
    ) -> (String, String?) -> Void {
        { sheetFileId, audioFileId in
            start {
                await bindSheet(
                    key: key,
                    sheetFileId: sheetFileId,
                    audioFileId: audioFileId,
                    services: services
                )
            }
        }
    }

    @MainActor
    private static func discAction(
        key: String,
        services: ImportMappingServices
    ) -> (String, BridgeSheetDisc) -> Void {
        { sheetFileId, disc in
            start {
                await setSheetDisc(
                    key: key,
                    sheetFileId: sheetFileId,
                    disc: disc,
                    services: services
                )
            }
        }
    }

    /// Write a row's edited track back onto the row that commits it.
    @MainActor
    static func editTrack(
        key: String,
        track: BridgeRawTrackEdit,
        importStore: ImportStore
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mapping?.setTrack(track)
        }
    }

    /// Point a row at one of the folder's audio units. The row starts writing
    /// that audio because the editor is what says which audio a track's samples
    /// come from — core's reading of the folder produced the row, and this is
    /// the user overruling it.
    @MainActor
    static func chooseFile(
        key: String,
        trackId: String,
        audio: BridgeAudioFile,
        importStore: ImportStore
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            guard
                var track = candidate.mapping?.units
                    .compactMap(\.track)
                    .first(where: { $0.id == trackId })
            else { return }
            track.file = audio
            candidate.mapping?.setTrack(track)
        }
    }

    /// Drop a row the release names and this folder has nothing for. Nothing is
    /// persisted: the folder is unchanged, the release is simply imported
    /// without that track.
    @MainActor
    static func drop(key: String, trackId: String, importStore: ImportStore) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mapping?.removeTrack(id: trackId)
        }
    }

    /// Take a file out of the tracklist. Core persists the decision — it is a
    /// fact about the folder, so it survives re-picking a release and
    /// relaunching — and the table drops the file's rows here, because the only
    /// other way to refresh it is another read from core, which would discard
    /// the user's edits.
    @MainActor
    static func exclude(
        key: String,
        fileId: String,
        services: ImportMappingServices
    ) async {
        guard
            await writeRole(
                key: key,
                fileId: fileId,
                choice: .notATrack,
                services: services
            )
        else { return }
        services.importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mapping?.removeFile(id: fileId)
        }
    }

    /// Put a file in a role, or put it back. Core persists it and drops the
    /// candidate's stored identify verdict; the table is re-read because a
    /// file that has changed jobs is a different set of rows, and there is no
    /// row to put back from here.
    @MainActor
    static func setRole(
        key: String,
        fileId: String,
        choice: BridgeFileRoleChoice,
        services: ImportMappingServices
    ) async {
        guard
            await writeRole(
                key: key,
                fileId: fileId,
                choice: choice,
                services: services
            )
        else { return }
        await readMapping(key: key, services: services)
    }

    /// Name the audio `sheetFileId` describes, or clear it with `nil`. Core
    /// persists the decision and drops the candidate's stored identify verdict.
    ///
    /// The table is re-read, because a binding changes what the folder's audio
    /// *is*: one container becomes a dozen entries. What comes back is for a
    /// different set of rows than the one the user was editing, which is
    /// exactly why it replaces them.
    @MainActor
    static func bindSheet(
        key: String,
        sheetFileId: String,
        audioFileId: String?,
        services: ImportMappingServices
    ) async {
        do {
            try await services.importer.setSheetBinding(
                key,
                sheetFileId,
                audioFileId
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            services.onError(
                String(
                    localized:
                        "Couldn't change what \(sheetFileId) describes: \(error.displayLine)"
                )
            )
            return
        }
        await readMapping(key: key, services: services)
    }

    /// Say which disc of the release a sheet's entries are, or take them out of
    /// the tracklist. Re-shapes the tracklist exactly as a binding does, so the
    /// table is re-read the same way.
    @MainActor
    static func setSheetDisc(
        key: String,
        sheetFileId: String,
        disc: BridgeSheetDisc,
        services: ImportMappingServices
    ) async {
        do {
            try await services.importer.setSheetDisc(key, sheetFileId, disc)
        }
        catch is CancellationError {
            return
        }
        catch {
            services.onError(
                String(
                    localized:
                        "Couldn't change which disc \(sheetFileId) is: \(error.displayLine)"
                )
            )
            return
        }
        await readMapping(key: key, services: services)
    }

    /// Write a file's role through to core. Returns whether the call landed.
    @MainActor
    private static func writeRole(
        key: String,
        fileId: String,
        choice: BridgeFileRoleChoice,
        services: ImportMappingServices
    ) async -> Bool {
        do {
            try await services.importer.setFileRole(key, fileId, choice)
            return true
        }
        catch is CancellationError {
            return false
        }
        catch {
            services.onError(
                String(
                    localized:
                        "Couldn't change what \(fileId) is: \(error.displayLine)"
                )
            )
            return false
        }
    }
}
