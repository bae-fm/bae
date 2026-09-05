import BaeKit
import SwiftUI

// MARK: - Candidate list and main pane

extension ImportView {
    var candidateList: some View {
        ImportCandidateListContent(
            importStore: importStore,
            listSlot: listSlot,
            selectedKeys: candidateSelectionBinding,
            onAddFolder: {
                uiStore.setImportFolderPickerPresented(true)
            },
            onRemoveFolder: { path in removeWatchedFolder(path) },
            onRefreshFolder: { folder in refreshWatchedFolder(folder) },
            onReleaseDecision: { key, decision in
                setFolderReleaseDecision(key, decision)
            },
            onSkip: { key, skipped in setCandidateSkipped(key, skipped) },
            onImportSelected: importReadyCandidates,
        )
    }

    // MARK: - Main pane

    /// The mapping pane for the selected candidate: metadata source, mapping
    /// table, and commit bar.
    func mainPane(for candidate: Candidate) -> some View {
        CandidateRuntimeReader(key: candidate.key) { runtime in
            mappingPane(for: candidate, runtime: runtime)
        }
        .id(candidate.key)
    }

    @ViewBuilder
    private func mappingPane(
        for candidate: Candidate,
        runtime: BridgeCandidateRuntimeSnapshot?
    ) -> some View {
        switch candidate.row?.importStatus {
        case .complete(let releaseId, let albumId):
            let releaseEditor = releaseEditor
            let mappingActions = mappingActions(for: candidate)
            ImportedReleasePane(
                candidate: candidate,
                releaseId: releaseId,
                albumId: albumId,
                seedReleaseEdit: releaseEditor.seedReleaseEdit,
                saveReleaseEdit: releaseEditor
                    .updateReleaseMetadataUserEdit,
                resetReleaseEdit: releaseEditor
                    .resetReleaseEditToSource,
                changeCover: releaseEditor.changeCover,
                fetchRemoteCovers: {
                    try await releaseEditor.fetchRemoteCovers(
                        .release(releaseId: $0)
                    )
                },
                onViewInLibrary: { uiStore.navigateToAlbum($0) },
                onOpenImages: mappingActions.openImages,
                onOpenDocument: mappingActions.openDocument,
                onPlayTrack: { index in
                    playback.playRelease(
                        releaseId,
                        UInt32(index),
                        false
                    )
                }
            )
        case .importing:
            let mappingActions = mappingActions(for: candidate)
            ImportingCandidatePane(
                candidate: candidate,
                runtime: runtime,
                coverContent: candidate.cover?.thumbnailContent,
                onOpenImages: mappingActions.openImages,
                onOpenDocument: mappingActions.openDocument,
                onPreview: mappingActions.preview,
                onStopPreview: mappingActions.stopPreview,
                previewingTarget: importStore.previewState.active?.target
            )
        case .error, nil:
            preparingMappingPane(for: candidate, runtime: runtime)
        }
    }

    private func preparingMappingPane(
        for candidate: Candidate,
        runtime: BridgeCandidateRuntimeSnapshot?
    ) -> some View {
        ImportMappingPane(
            candidate: candidate,
            runtime: runtime,
            bindingOptions: sheetBindingOptions,
            previewingTarget: importStore.previewState.active?.target,
            libraryStatus: candidate.pickedLibraryStatus,
            hasCoverOptions: hasCoverOptions(candidate),
            coverContent: candidate.cover?.thumbnailContent,
            editActions: editActions(for: candidate),
            editingCommands: editingCommands,
            endEditing: commitAndEndEditing,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned,
            mappingActions: mappingActions(for: candidate),
            commitActions: ImportCommitActions(
                confirmImport: { commitConfirmedImport(candidate: candidate) },
                mergeArtists: {
                    mergeArtistIdentityConflict(
                        candidate: candidate,
                        keeping: $0
                    )
                },
                viewInLibrary: { uiStore.navigateToAlbum($0) },
            ),
            onPresentMetadata: {
                presentMetadata($0, for: candidate)
            },
            onReadFileTags: {
                ImportMappingFlow.loadFileTagsPreview(
                    key: candidate.key,
                    services: mappingServices
                )
            },
            onUseFileTags: {
                ImportMappingFlow.useFileTags(
                    key: candidate.key,
                    services: mappingServices
                )
            },
            onClearMetadata: {
                ImportMappingFlow.clearMetadata(
                    key: candidate.key,
                    services: mappingServices
                )
            },
            onEditCover: { presentCoverPicker(for: candidate) },
            onSelectCover: { selection in
                selectCover(selection, for: candidate)
            },
            onNavigateToPlacement: { key in
                listSlot.requestCandidateReveal(key)
            },
        )
        .animation(nil, value: uiStore.selectedFolderCandidates)
        // Keyed on the folder's files: what a sheet may be bound to changes
        // when the folder's audio does, and nothing else moves it. The table
        // itself needs no read — it rides the candidate's own value.
        .task(id: candidate.key + fileNames(candidate)) {
            await loadSheetBindingOptions(for: candidate)
        }
    }

    /// Where one album-level field's typed value goes: a row under this
    /// candidate, written as the field is left.
    private func editActions(for candidate: Candidate) -> ReleaseFieldWriter {
        let key = candidate.key
        return ReleaseFieldWriter(
            setField: { field, value in
                await saveCandidateEdit {
                    try await importer.setCandidateEditField(key, field, value)
                }
            },
            setAlbumArtists: { assignments in
                await saveCandidateEdit {
                    try await importer.setCandidateAlbumArtists(
                        key,
                        assignments
                    )
                }
            }
        )
    }

    private func saveCandidateEdit(
        _ save: @escaping @MainActor () async throws -> Void
    ) async {
        do {
            try await save()
        }
        catch is CancellationError {}
        catch {
            if let line = error.displayLine {
                uiStore.showError(
                    String(localized: "Couldn't save that change: \(line)")
                )
            }
        }
    }

    /// Whether the cover is worth opening a picker for: the picked release's
    /// remote art, or artwork found in the folder.
    private func hasCoverOptions(_ candidate: Candidate) -> Bool {
        candidate.release != nil || !candidate.files.images.isEmpty
    }

    /// Every file the candidate holds, in one string — the identity the
    /// binding-offer read is refreshed on.
    private func fileNames(_ candidate: Candidate) -> String {
        candidate.files.files.map(\.file.name).joined(separator: "\u{0}")
    }

    private func presentCoverPicker(for candidate: Candidate) {
        let key = candidate.key
        uiStore.presentModal {
            CoverPickerFrame {
                CoverPickerView(
                    remoteCoverArts: candidate.release?.coverArt ?? [],
                    localArtwork: candidate.files.images,
                    selectedCover: candidate.cover,
                    fetchRemoteCovers: {
                        try await releaseEditor.fetchRemoteCovers(
                            .candidate(candidateKey: key)
                        )
                    },
                    onFindRelease: {
                        uiStore.dismissModal()
                        presentMetadata(.findOnline, for: candidate)
                    },
                    onSelect: { selection in
                        try await importer.setCandidateCover(
                            key,
                            selection.selection
                        )
                    },
                    onDone: { uiStore.dismissModal() },
                )
            }
        }
    }

    private func selectCover(
        _ selection: BridgeCoverSelection,
        for candidate: Candidate
    ) {
        selectCover(selection, forKey: candidate.key)
    }

    private func selectCover(
        _ selection: BridgeCoverSelection,
        forKey key: String
    ) {
        Task { @MainActor in
            do {
                try await importer.setCandidateCover(key, selection)
            }
            catch is CancellationError {}
            catch {
                if let line = error.displayLine {
                    uiStore.showError(
                        String(
                            localized: "Couldn't change the cover: \(line)"
                        )
                    )
                }
            }
        }
    }
}
