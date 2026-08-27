import BaeKit
import Foundation

/// The services the mapping pane's actions drive.
struct ImportMappingServices {
    let importer: Importer
    /// Where a failed command's line lands, on the candidate whose pane ran
    /// it. Nothing about the table is held here — every edit is a row core
    /// stores, and the per-candidate read redraws from it.
    let importStore: ImportStore
    let previewAudio: PreviewAudio
    /// Show a document (a log, a text file, a track sheet) in the viewer: its
    /// name, then its path on disk.
    let openDocument: (String, String) -> Void
    /// Show the folder's images in the lightbox: the gallery's images, then
    /// the path of the one that was clicked.
    let openImages: ([BridgeMappingImage], String) -> Void
    /// Surface a failed call to the user. Prose the caller already localized.
    let onError: (String) -> Void
}

/// What the mapping table's controls do to a candidate.
///
/// Separate from the views so the pane's behaviour — what excluding a file
/// leaves behind, what naming a row changes, what assigning a cue to a disc
/// re-reads — is exercised without a view hierarchy.
enum ImportMappingFlow {
    /// Present one side of the metadata section. File Tags is also a stored
    /// identity choice; Lookup only navigates to the inline lookup surface.
    @MainActor
    static func setIdentity(
        _ identity: ImportIdentity,
        for candidate: Candidate,
        services: ImportMappingServices
    ) {
        services.importStore.presentIdentity(identity, forKey: candidate.key)
        guard identity == .unknown else { return }
        ImportSearchFlow.decideIdentity(
            importer: services.importer,
            importStore: services.importStore,
            key: candidate.key,
            pick: .unknown
        )
    }

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
            openImages: services.openImages,
            preview: { path in services.previewAudio.previewPlay(path) },
            stopPreview: { services.previewAudio.previewStop() },
            editTrack: { track in
                start {
                    await editTrack(
                        key: key,
                        track: track,
                        services: services
                    )
                }
            },
            chooseFile: { trackId, audio in
                start {
                    await chooseFile(
                        key: key,
                        trackId: trackId,
                        audio: audio,
                        services: services
                    )
                }
            },
            drop: { trackId in
                start {
                    await drop(key: key, trackId: trackId, services: services)
                }
            },
            exclude: { fileId in
                start {
                    _ = await writeRole(
                        key: key,
                        fileId: fileId,
                        choice: .notATrack,
                        services: services
                    )
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

    /// Put a file in a role, or put it back.    /// Put a file in a role, or put it back. Core persists it and drops the
    /// candidate's stored identify verdict; a file that has changed jobs is a
    /// different set of rows, and the per-candidate read draws them.
    @MainActor
    static func setRole(
        key: String,
        fileId: String,
        choice: BridgeFileRoleChoice,
        services: ImportMappingServices
    ) async {
        _ = await writeRole(
            key: key,
            fileId: fileId,
            choice: choice,
            services: services
        )
    }

    /// Name the audio `sheetFileId` describes, or clear it with `nil`. Core
    /// persists the decision and drops the candidate's stored identify verdict.
    ///
    /// A binding changes what the folder's audio *is*: one container becomes a
    /// dozen entries. The rows the person was editing are a different set
    /// afterwards, which is why core drops their row edits with the binding.
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
            // No line means a cancellation, which has nothing to report; the
            // write still did not land, so the read below is still skipped.
            if let line = error.displayLine {
                services.onError(
                    String(
                        localized:
                            "Couldn't change what \(sheetFileId) describes: \(line)"
                    )
                )
            }
            return
        }
    }

    /// Say which disc of the release a sheet's entries are, or take them out of
    /// the tracklist. Re-shapes the tracklist exactly as a binding does.
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
            if let line = error.displayLine {
                services.onError(
                    String(
                        localized:
                            "Couldn't change which disc \(sheetFileId) is: \(line)"
                    )
                )
            }
            return
        }
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
            if let line = error.displayLine {
                services.onError(
                    String(
                        localized:
                            "Couldn't change what \(fileId) is: \(line)"
                    )
                )
            }
            return false
        }
    }
}
