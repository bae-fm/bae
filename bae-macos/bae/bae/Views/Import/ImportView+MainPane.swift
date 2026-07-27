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

    func mainPane(for candidate: Candidate) -> some View {
        ImportMainPane(
            files: candidate.files,
            onOpenGallery: { index in
                let images = candidate.files.images
                guard images.indices.contains(index) else {
                    return
                }
                let tappedPath = images[index].file.localPath
                let items = images.map { file in
                    LightboxItem(
                        id: file.file.localPath,
                        label: file.file.name,
                        source: .local(path: file.file.localPath)
                    )
                }
                uiStore.presentLightbox(items: items, preferring: tappedPath)
            },
            onOpenDocument: { name, text in
                documentContent = (name: name, text: text)
            },
            onPreviewAudio: { path in
                previewAudio.previewPlay(path)
            },
            onError: { uiStore.showError($0) },
            previewState: importStore.previewState,
        ) {
            resultPane(for: candidate)
        }
        .animation(nil, value: uiStore.selectedFolderCandidate)
    }

    /// True while the confirm pane is docked — during detail load and while
    /// confirming.
    private func paneOpen(_ candidate: Candidate) -> Bool {
        candidate.mode == .loadingDetail || candidate.mode == .confirming
    }

    /// Search/results above, the confirm pane docked at the bottom. The results
    /// stay visible and scrollable; the pane slides up when a pressing is
    /// picked and is drag-resizable.
    private func resultPane(for candidate: Candidate) -> some View {
        let open = paneOpen(candidate)
        return ImportResultPane(
            open: open,
            onClose: { closePane(candidate) },
            top: {
                searchAndResultsPane(
                    for: candidate,
                    selectedReleaseId: open
                        ? candidate.pickedReleaseId : nil
                )
            },
            pane: { paneContent(for: candidate) }
        )
    }

    /// Pane body: a loading spinner while the source detail loads, the
    /// editable confirm form once it's in, and nothing when the pane is closed
    /// (it's clipped to zero height then).
    @ViewBuilder
    private func paneContent(for candidate: Candidate) -> some View {
        switch candidate.mode {
        case .loadingDetail:
            ProgressView("Loading release details...")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .confirming:
            confirmationView(for: candidate)
        case .identifying:
            Color.clear
        }
    }

    /// Close the pane and drop the pick cluster so reopening the identify
    /// surface leaves no stale claim or file-tag edit behind.
    private func closePane(_ candidate: Candidate) {
        importStore.mutateCandidate(forKey: candidate.key) { c in
            c.mode = .identifying
            c.releaseDetailBridge = nil
            c.claim = nil
            c.pickedReleaseId = nil
            c.identityChoice = nil
            c.editValues = nil
        }
    }
}
