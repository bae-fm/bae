import BaeKit
import SwiftUI

/// The metadata slot: the candidate's one editable draft, or the Find online
/// page a person opens to identify it.
struct ImportMetadataSourceSection: View {
    let candidate: Candidate
    /// Whether the candidate can be edited and identified now. A candidate
    /// that cannot still has its records: a link to a catalog page is not an
    /// edit, so those stay live while everything else in the slot waits.
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
                editActions: editActions,
                editingCommands: editingCommands,
                commit: commit,
                sourceActions: ImportReleaseSourceActions(
                    identifyAutomatically: onIdentify,
                    searchForRelease: onSearchForRelease,
                    reset: onReset,
                    resetToFileMetadata: onResetToFileMetadata,
                    clearMetadata: onClearMetadata
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

/// The online browser reads its candidate from the store because its form
/// bindings and result application write that same candidate. The pane carries
/// its own title row, so the slot mounts it whole.
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
                        selectedReleaseId: candidate.pickedRelease?.key,
                        runtime: runtime,
                        openSection: candidate.session.findOnlineSection,
                        onOpenSection: {
                            onMovePane(.openSection(section: $0))
                        },
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
                            provenance: pressing.provenance
                        )
                    }
                )
            }
            .frame(maxWidth: .infinity)
            .formGroupCard()
        }
    }
}
