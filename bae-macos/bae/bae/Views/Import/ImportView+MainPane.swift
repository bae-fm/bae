import BaeKit
import SwiftUI
import os.log

private let mainPaneLogger = Logger.bae("ImportMainPane")

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
        ImportMappingPane(
            candidate: candidate,
            bindingOptions: sheetBindingOptions,
            previewingPath: importStore.previewState.active?.path,
            libraryStatus: libraryStatus(for: candidate),
            hasCoverOptions: hasCoverOptions(candidate),
            coverContent: candidate.selectedCover?.thumbnailContent,
            editor: editorBinding(for: candidate),
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
                ImportSearchFlow.decideIdentity(
                    importer: importer,
                    importStore: importStore,
                    key: candidate.key,
                    pick: .release(
                        source: result.source,
                        releaseId: result.releaseId,
                        claim: .exact
                    )
                )
            },
            onToggleSignal: { signal in
                importer.toggleSignalForCandidate(candidate.key, signal)
            },
            onEditCover: { presentCoverPicker(for: candidate) },
            onSetClaimLevel: { level in
                setClaimLevel(level, for: candidate)
            },
        )
        .animation(nil, value: uiStore.selectedFolderCandidates)
        // Keyed on the folder's files, not on the candidate: what a sheet may
        // be bound to changes when the folder's audio does, and the identify
        // phase's table changes with it for the same reason. Neither changes
        // when a binding or a role does — those commands return the updated
        // table themselves, which is also what keeps this from overwriting a table
        // the user has been editing.
        .task(id: candidate.key + fileNames(candidate)) {
            if candidate.identityChoice == nil {
                await ImportMappingFlow.readCandidateMapping(
                    key: candidate.key,
                    services: mappingServices
                )
            }
            await loadSheetBindingOptions(for: candidate)
        }
    }

    /// Claim the candidate's picked release at `level`. It re-picks the same
    /// release, which is what stores the level: the claim is part of the
    /// decision, not a second thing to persist. Nothing picked is nothing to
    /// claim, and the claim line the control lives in is not drawn then.
    private func setClaimLevel(
        _ level: BridgeClaimLevel,
        for candidate: Candidate
    ) {
        guard let pick = candidate.pick else {
            mainPaneLogger.debug(
                "no release picked for \(candidate.key); nothing to claim"
            )
            return
        }
        ImportSearchFlow.decideIdentity(
            importer: importer,
            importStore: importStore,
            key: candidate.key,
            pick: .release(
                source: pick.source,
                releaseId: pick.releaseId,
                claim: level
            )
        )
    }

    /// The album-level editor, or `nil` while nothing has seeded one — the pane
    /// leaves the release fields and the commit bar off until then.
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

    /// Open the release editor: the search pane, reached the way an editor is
    /// reached.
    func presentSearch(for candidate: Candidate) {
        uiStore.presentModal {
            ImportSearchSheet(candidateKey: candidate.key)
        }
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
