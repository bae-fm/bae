import BaeKit
import SwiftUI

/// The metadata slot: the candidate's one editable draft, or the Find online
/// page a person opens to identify it.
struct ImportMetadataSourceSection: View {
    let candidate: Candidate
    /// Whether the candidate can be edited and identified now; its catalog
    /// links stay live either way.
    let actionable: Bool
    let runtime: BridgeCandidateRuntimeSnapshot?
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    let editActions: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    let endEditing: @MainActor () async -> Void
    let commit: ImportCommitControls?
    /// Move the pane as the person asked.
    let onMovePane: (BridgePaneMove) -> Void
    /// Show identification's results for this candidate, as core decides.
    let onIdentify: () -> Void
    /// Open the pane on its typed search, starting nothing.
    let onSearchForRelease: () -> Void
    let onReset: () -> Void
    let onResetToFileMetadata: () -> Void
    let onClearMetadata: () -> Void
    let onUnlink: () -> Void
    let onEditCover: () -> Void
    let onSelectCover: (BridgeCoverSelection) -> Void

    var body: some View {
        Group {
            switch candidate.metadataPresentation {
            case .draft:
                draft
            case .findOnline:
                ImportOnlineMetadataBrowser(
                    candidateKey: candidate.key,
                    runtime: runtime,
                    endEditing: endEditing,
                    onMovePane: onMovePane
                )
                .disabled(!actionable)
            }
        }
    }

    @ViewBuilder
    private var draft: some View {
        if let edit = candidate.edit {
            ImportReleaseHeader(
                releaseSummary: ImportReleaseSummary(
                    candidate: candidate,
                    editValues: edit
                ),
                actionable: actionable,
                isReading: isReading,
                coverContent: coverContent,
                hasCoverOptions: hasCoverOptions,
                editValues: edit,
                records: candidate.records,
                releaseLink: candidate.releaseLink,
                editActions: editActions,
                editingCommands: editingCommands,
                commit: commit,
                sourceActions: ImportReleaseSourceActions(
                    identifyAutomatically: onIdentify,
                    searchForRelease: onSearchForRelease,
                    reset: onReset,
                    resetToFileMetadata: onResetToFileMetadata,
                    clearMetadata: onClearMetadata,
                    unlink: onUnlink
                ),
                localCoverSelections: candidate.localCoverSelections,
                onEditCover: onEditCover,
                onSelectCover: onSelectCover
            )
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, minHeight: 180)
        }
    }
}

/// Find online for the candidate, read from the store that its form and picks
/// write.
private struct ImportOnlineMetadataBrowser: View {
    let candidateKey: String
    let runtime: BridgeCandidateRuntimeSnapshot?
    let endEditing: @MainActor () async -> Void
    let onMovePane: (BridgePaneMove) -> Void

    @Environment(Importer.self)
    private var importer
    @Environment(ImportStore.self)
    private var importStore
    @Environment(\.openSettings)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation

    var body: some View {
        if let candidate = importStore.candidate(forKey: candidateKey) {
            CandidateSignalsReader(key: candidateKey) { signals in
                ImportSearchFlow.buildSearchPane(
                    services: ImportSearchFlow.ImportServices(
                        importer: importer,
                        importStore: importStore
                    ),
                    input: ImportSearchFlow.SearchPaneInput(
                        candidate: candidate,
                        key: candidateKey,
                        selectedReleaseId: candidate.releaseLink?.pressing?
                            .record.key,
                        runtime: runtime,
                        openSection: candidate.session.findOnlineSection,
                        onOpenSection: {
                            onMovePane(.openSection(section: $0))
                        },
                        onKeepOwnDraft: keepOwnDraft,
                        onLinkSharedAlbum: linkSharedAlbum,
                        onCancelIdentification: cancelIdentification,
                        liveSignals: signals
                    ),
                    openSettings: {
                        settingsNavigation.open(
                            .importing,
                            present: { openSettings() }
                        )
                    },
                    onBack: { onMovePane(.back) },
                    onSelect: { pressing in
                        ImportSearchFlow.applyMetadata(
                            importer: importer,
                            importStore: importStore,
                            endEditing: endEditing,
                            key: candidateKey,
                            application: .pick(pressing.link)
                        )
                    }
                )
            }
            .frame(maxWidth: .infinity)
            .card()
        }
    }

    /// Keep the candidate's own draft over what its lookup offered. Its
    /// failure is stated on the pane, from what core stored.
    private func keepOwnDraft() {
        Task { @MainActor in
            await endEditing()
            do { _ = try await importer.keepCandidateDraft(candidateKey) }
            catch is CancellationError {}
            catch { importStore.reportFailure(error) }
        }
    }

    /// Take the candidate off the identification queue; core puts the pane
    /// back on the draft, as Back does.
    private func cancelIdentification() {
        Task { @MainActor in
            do {
                try await importer.cancelCandidateIdentification(candidateKey)
            }
            catch is CancellationError {}
            catch { importStore.reportFailure(error) }
        }
    }

    /// Link the candidate to the album the offered pressings are of, its
    /// pressing unknown. Its failure is stated on the pane, from what core
    /// stored.
    private func linkSharedAlbum() {
        Task { @MainActor in
            await endEditing()
            do {
                _ = try await importer.linkCandidateSharedAlbum(candidateKey)
            }
            catch is CancellationError {}
            catch { importStore.reportFailure(error) }
        }
    }
}
