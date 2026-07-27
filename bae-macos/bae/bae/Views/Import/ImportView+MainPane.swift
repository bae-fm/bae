import BaeKit
import SwiftUI

// MARK: - Candidate list and main pane

extension ImportView {
    var candidateList: some View {
        ImportCandidateListContent(
            importStore: importStore,
            selectedKey: candidateSelectionBinding,
            onAddFolder: { pickFolderAndAdd() },
            onRemoveFolder: { path in removeWatchedFolder(path) },
            onSkip: { key, skipped in setCandidateSkipped(key, skipped) },
            onImportSelected: { keys in importReadyCandidates(keys) },
        )
    }

    // MARK: - Main pane

    /// The mapping pane for the selected candidate: release header, file roles,
    /// track slots, commit bar.
    func mainPane(for candidate: Candidate) -> some View {
        ImportMappingPane(
            candidate: candidate,
            model: ImportMappingModel(
                files: candidate.files,
                slots: candidate.slots,
                edit: candidate.editValues
            ),
            bindingOptions: sheetBindingOptions,
            previewingPath: importStore.previewState.active?.path,
            libraryStatus: libraryStatus(for: candidate),
            hasCoverOptions: hasCoverOptions(candidate),
            coverSource: candidate.selectedCover.map {
                ImageLoader.Source(bridge: $0.thumbnailSource)
            },
            editor: editorBinding(for: candidate),
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            roleActions: roleActions(for: candidate),
            slotActions: slotActions(for: candidate),
            commitActions: ImportCommitActions(
                confirmImport: { commitConfirmedImport(candidate: candidate) },
                viewInLibrary: { uiStore.navigateToAlbum($0) },
            ),
            onFindRelease: {
                uiStore.presentModal {
                    ImportSearchSheet(candidateKey: candidate.key)
                }
            },
            onEditCover: { presentCoverPicker(for: candidate) },
        )
        .animation(nil, value: uiStore.selectedFolderCandidate)
        // Keyed on the folder's files, not on the candidate: what a sheet may
        // be bound to changes when the folder's audio does, and not when a
        // binding does.
        .task(id: candidate.key + fileNames(candidate)) {
            await loadSheetBindingOptions(for: candidate)
        }
    }

    /// The live editor, or `nil` while nothing has seeded one — the pane leaves
    /// the slot table and the commit bar off until then.
    private func editorBinding(
        for candidate: Candidate
    ) -> Binding<BridgeRawReleaseEdit>? {
        guard candidate.editValues != nil else { return nil }
        return ImportSearchFlow.makeEditValuesBinding(
            importStore: importStore,
            key: candidate.key,
            candidate: candidate
        )
    }

    private func libraryStatus(
        for candidate: Candidate
    ) -> BridgeLibraryStatus? {
        candidate.releaseDetailBridge
            .flatMap { candidate.libraryStatuses[$0.releaseId] }
    }

    /// Whether the cover is worth opening a picker for: the picked release's
    /// remote art, or artwork found in the folder.
    private func hasCoverOptions(_ candidate: Candidate) -> Bool {
        !(candidate.releaseDetailBridge?.coverArt ?? []).isEmpty
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
                remoteCoverArts: candidate.releaseDetailBridge?.coverArt ?? [],
                localArtwork: candidate.files.images,
                selectedCover: candidate.selectedCover,
                onSelect: { selection in
                    importStore.mutateCandidate(forKey: key) {
                        $0.selectedCover = selection
                    }
                    uiStore.dismissModal()
                },
                onDone: { uiStore.dismissModal() },
            )
            .frame(width: 600, height: 500)
        }
    }
}
