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

    /// The mapping pane for the selected candidate: identity, the mapping
    /// table, commit bar.
    func mainPane(for candidate: Candidate) -> some View {
        CandidateRuntimeReader(key: candidate.key) { runtime in
            mappingPane(for: candidate, runtime: runtime)
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
            previewingPath: importStore.previewState.active?.path,
            libraryStatus: candidate.pickedLibraryStatus,
            hasCoverOptions: hasCoverOptions(candidate),
            coverContent: candidate.cover?.thumbnailContent,
            editActions: editActions(for: candidate),
            storageCloud: $storageCloud,
            storagePinned: $storagePinned,
            mappingActions: mappingActions(for: candidate),
            commitActions: ImportCommitActions(
                confirmImport: { commitConfirmedImport(candidate: candidate) },
                viewInLibrary: { uiStore.navigateToAlbum($0) },
            ),
            onSetIdentity: { setIdentity($0, for: candidate) },
            onFindRelease: { presentSearch(for: candidate) },
            onPickRelease: { result in
                ImportSearchFlow.selectMetadataSeed(
                    importer: importer,
                    importStore: importStore,
                    key: candidate.key,
                    seed: .externalRelease(
                        source: result.source,
                        releaseId: result.releaseId
                    )
                )
            },
            onEditCover: { presentCoverPicker(for: candidate) },
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
            { field, value in
                saveCandidateEdit {
                    try await importer.setCandidateEditField(key, field, value)
                }
            },
            setAlbumArtists: { assignments in
                saveCandidateEdit {
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
    ) {
        Task { @MainActor in
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

    /// Open the release editor: the search pane, reached the way an editor is
    /// reached.
    func presentSearch(for candidate: Candidate) {
        let presentation = ModalPresentation()
        uiStore.presentModal(presentation: presentation) {
            ImportSearchSheet(
                candidateKey: candidate.key,
                presentation: presentation
            )
        }
    }

    private func presentCoverPicker(for candidate: Candidate) {
        let key = candidate.key
        uiStore.presentModal {
            CoverPickerView(
                remoteCoverArts: candidate.release?.coverArt ?? [],
                localArtwork: candidate.files.images,
                selectedCover: candidate.cover,
                onSelect: { selection in
                    Task { @MainActor in
                        do {
                            try await importer.setCandidateCover(
                                key,
                                selection.selection
                            )
                        }
                        catch is CancellationError {}
                        catch {
                            if let line = error.displayLine {
                                uiStore.showError(
                                    String(
                                        localized:
                                            "Couldn't change the cover: \(line)"
                                    )
                                )
                            }
                        }
                    }
                    uiStore.dismissModal()
                },
                onDone: { uiStore.dismissModal() },
            )
            .frame(width: 600, height: 500)
        }
    }
}
