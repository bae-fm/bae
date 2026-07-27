import BaeKit
import Foundation

/// The services the mapping pane's actions drive.
struct ImportMappingServices {
    let importer: Importer
    let importStore: ImportStore
    let previewAudio: PreviewAudio
    /// Surface a failed call to the user. Prose the caller already localized.
    let onError: (String) -> Void
}

/// What the mapping pane's controls do to a candidate.
///
/// Separate from the views so the pane's behaviour — what excluding a file
/// leaves behind, what naming a row changes, what a slot's play button reaches
/// — is exercised without a view hierarchy.
enum ImportMappingFlow {
    @MainActor
    static func slotActions(
        key: String,
        services: ImportMappingServices
    ) -> ImportSlotActions {
        ImportSlotActions(
            preview: { path in services.previewAudio.previewPlay(path) },
            stopPreview: { services.previewAudio.previewStop() },
            chooseFile: { index, audio in
                chooseFile(
                    key: key,
                    index: index,
                    audio: audio,
                    importStore: services.importStore
                )
            },
            drop: { index in
                drop(key: key, index: index, importStore: services.importStore)
            },
            exclude: { fileId in
                Task { @MainActor in
                    await exclude(key: key, fileId: fileId, services: services)
                }
            },
        )
    }

    /// Bind a row to one of the folder's audio units. The row flips to paired
    /// because the editor is what says which audio a track's samples come from
    /// — the slot table's variant is core's reading of the folder, and this is
    /// the user overruling it.
    @MainActor
    static func chooseFile(
        key: String,
        index: Int,
        audio: BridgeAudioFile,
        importStore: ImportStore
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            guard candidate.editValues?.tracks.indices.contains(index) == true
            else { return }
            candidate.editValues?.tracks[index].file = audio
        }
    }

    /// Drop a slot the source names and this folder has nothing for. Nothing is
    /// persisted: the folder is unchanged, the release is simply imported
    /// without that track.
    @MainActor
    static func drop(key: String, index: Int, importStore: ImportStore) {
        importStore.mutateCandidate(forKey: key) { candidate in
            guard let edit = candidate.editValues else { return }
            let removal = ImportMappingModel.dropping(
                rowAt: index,
                slots: candidate.slots,
                edit: edit
            )
            candidate.slots = removal.slots
            candidate.editValues = removal.edit
        }
    }

    /// Take a file out of the tracklist. Core persists the decision — it is a
    /// fact about the folder, so it survives re-picking a release and
    /// relaunching — and the slot table catches up here, because the only other
    /// way to refresh it is another prefetch, which would discard the user's
    /// edits.
    @MainActor
    static func exclude(
        key: String,
        fileId: String,
        services: ImportMappingServices
    ) async {
        guard
            await setRole(
                key: key,
                fileId: fileId,
                choice: .notATrack,
                services: services
            )
        else { return }
        services.importStore.mutateCandidate(forKey: key) { candidate in
            guard let slots = candidate.slots, let edit = candidate.editValues
            else { return }
            let removal = ImportMappingModel.excluding(
                fileId: fileId,
                from: slots,
                edit: edit
            )
            candidate.slots = removal.slots
            candidate.editValues = removal.edit
        }
    }

    /// Put a file in a role, or put it back. Core persists it, drops the
    /// candidate's stored identify verdict, and the candidate invalidation
    /// brings back the new roles. Returns whether the call landed.
    @MainActor
    @discardableResult
    static func setRole(
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
