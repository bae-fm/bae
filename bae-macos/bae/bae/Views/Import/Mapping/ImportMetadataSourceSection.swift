import BaeKit
import SwiftUI

/// The metadata slot: the candidate's one editable draft or one temporary
/// browser that can replace it from an online release or file tags.
struct ImportMetadataSourceSection: View {
    let candidate: Candidate
    let runtime: BridgeCandidateRuntimeSnapshot?
    let fileTagsPreviewSummary: ImportReleaseSummary?
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    @Binding
    var detailsExpanded: Bool
    let editActions: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    let endEditing: @MainActor () async -> Void
    let commit: ImportCommitControls?
    let onPresent: (CandidateMetadataPresentation) -> Void
    let onReadFileTags: () -> Void
    let onUseFileTags: () -> Void
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
                    onBack: { onPresent(.draft) }
                )
            case .fileTags:
                fileTagsBrowser
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
                draftIsBlank: candidate.metadataDraftIsBlank,
                isReading: isReading,
                coverContent: coverContent,
                hasCoverOptions: hasCoverOptions,
                detailsExpanded: $detailsExpanded,
                editValues: edit,
                editActions: editActions,
                editingCommands: editingCommands,
                commit: commit,
                sourceActions: ImportReleaseSourceActions(
                    findOnline: { onPresent(.findOnline) },
                    useFileTags: { onPresent(.fileTags) },
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

    private var fileTagsBrowser: some View {
        VStack(alignment: .leading, spacing: 12) {
            browserHeader(
                title: coreString("ui.import.metadata.file_tags")
            )
            if let fileTagsPreviewSummary {
                HStack(alignment: .top, spacing: 12) {
                    ImportReleaseSummaryView(
                        summary: fileTagsPreviewSummary,
                        style: .card
                    )
                    ProgressView()
                        .controlSize(.small)
                        .opacity(isReading ? 1 : 0)
                    Button("Apply") { onUseFileTags() }
                        .buttonStyle(.borderedProminent)
                        .disabled(isReading)
                }
                .padding(14)
                .formGroupCard()
            }
            else if isReading {
                ProgressView("Reading file tags…")
                    .frame(maxWidth: .infinity, minHeight: 180)
                    .formGroupCard()
            }
            else {
                ContentUnavailableView(
                    "File tags could not be read",
                    systemImage: "tag.slash",
                    description: Text("Try reading the candidate again.")
                )
                .frame(maxWidth: .infinity, minHeight: 180)
                .overlay(alignment: .bottom) {
                    Button("Try again") { onReadFileTags() }
                        .buttonStyle(.bordered)
                        .padding()
                }
                .formGroupCard()
            }
        }
        .onAppear {
            if fileTagsPreviewSummary == nil, !isReading {
                onReadFileTags()
            }
        }
    }

    private func browserHeader(title: String) -> some View {
        HStack(spacing: 8) {
            Button {
                onPresent(.draft)
            } label: {
                Label("Back", systemImage: "chevron.left")
            }
            .buttonStyle(.link)
            Text(title)
                .font(.system(size: 13, weight: .semibold))
            Spacer()
        }
        .padding(.horizontal, 4)
    }
}

/// The online browser reads its candidate from the store because its form
/// bindings and result application write that same candidate.
private struct ImportOnlineMetadataBrowser: View {
    let candidateKey: String
    let runtime: BridgeCandidateRuntimeSnapshot?
    let endEditing: @MainActor () async -> Void
    let onBack: () -> Void

    @Environment(Importer.self)
    private var importer
    @Environment(ImportStore.self)
    private var importStore
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(\.openSettings)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation
    @State
    private var mode: BridgeDefaultFindOnlineMode = .automatic

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ZStack {
                Text("Find online")
                    .font(.system(size: 13, weight: .semibold))
                    .frame(maxWidth: .infinity)
                HStack {
                    Button {
                        onBack()
                    } label: {
                        Label("Back", systemImage: "chevron.left")
                    }
                    .buttonStyle(.link)
                    Spacer()
                }
            }
            .padding(.horizontal, 4)
            .padding(.bottom, 8)

            if let candidate = importStore.candidate(forKey: candidateKey) {
                CandidateSignalsReader(key: candidateKey) { signals in
                    ImportSearchFlow.buildSearchPane(
                        services: ImportSearchFlow.ImportServices(
                            importer: importer,
                            importStore: importStore,
                            configStore: configStore
                        ),
                        input: ImportSearchFlow.SearchPaneInput(
                            candidate: candidate,
                            key: candidateKey,
                            selectedReleaseId: candidate.pickedRelease?
                                .releaseId,
                            runtime: runtime,
                            liveSignals: signals
                        ),
                        mode: $mode,
                        openSettings: {
                            settingsNavigation.open(
                                .discogs,
                                present: { openSettings() }
                            )
                        },
                        onSelect: { result in
                            ImportSearchFlow.applyMetadata(
                                importer: importer,
                                importStore: importStore,
                                endEditing: endEditing,
                                key: candidateKey,
                                provenance: .externalRelease(
                                    source: result.source,
                                    releaseId: result.releaseId
                                ),
                                onConfirmed: {
                                    Task { @MainActor in onBack() }
                                }
                            )
                        }
                    )
                }
                .frame(maxWidth: .infinity)
                .formGroupCard()
            }
        }
        .onAppear {
            mode = configStore.config.defaultFindOnlineMode
        }
    }
}
