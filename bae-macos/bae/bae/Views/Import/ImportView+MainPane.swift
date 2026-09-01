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
            onImportSelected: { keys in importReadyCandidates(keys) },
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
        .onAppear {
            establishMetadataDetailsInitialState(for: candidate)
        }
        .onChange(of: candidate.metadataDraftIsBlank) { _, _ in
            establishMetadataDetailsInitialState(for: candidate)
        }
        .onChange(of: candidate.metadataProvenance) { _, provenance in
            if case .externalRelease = provenance {
                metadataDetailsState.externalReleaseApplied(
                    for: candidate.key
                )
            }
        }
    }

    private func mappingPane(
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
            detailsExpanded: metadataDetailsExpanded(for: candidate),
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
        !(candidate.release?.coverArt ?? []).isEmpty
            || !candidate.files.images.isEmpty
    }

    /// Every file the candidate holds, in one string — the identity the
    /// binding-offer read is refreshed on.
    private func fileNames(_ candidate: Candidate) -> String {
        candidate.files.files.map(\.file.name).joined(separator: "\u{0}")
    }

    private func presentCoverPicker(for candidate: Candidate) {
        let key = candidate.key
        uiStore.presentModal {
            CoverPickerView(
                remoteCoverArts: candidate.release?.coverArt ?? [],
                localArtwork: candidate.files.images,
                selectedCover: candidate.cover,
                onSelect: { selection in
                    selectCover(selection.selection, forKey: key)
                    uiStore.dismissModal()
                },
                onDone: { uiStore.dismissModal() },
            )
            .frame(width: 600, height: 500)
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
