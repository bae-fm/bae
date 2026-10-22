import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("ReIdentifySheet")

/// The candidate key a release's re-identify session runs under.
enum ReIdentifyKey {
    static func make(forReleaseId releaseId: String) -> String {
        "reidentify:\(releaseId)"
    }
}

/// Re-runs identification on a library release's files through the import
/// search pane, applies the pick to the release's edit session — every value
/// it gives, held until the save — and opens the editor on the result.
struct ReIdentifySheet: View {
    let releaseId: String
    let displayName: String
    let onClose: () -> Void

    /// The widest a prompt's or failure's message runs.
    private static let messageWidth: CGFloat = 420

    @Environment(Importer.self)
    private var importer
    @Environment(ReleaseEditor.self)
    private var releaseEditor
    @Environment(ImportStore.self)
    private var importStore
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.openSettingsWindow)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation

    @State
    private var phase: Phase = .identifying
    /// Which section of the search page is open. The sheet's own: a library
    /// release has no stored pane to keep it.
    @State
    private var openSection: BridgeFindOnlineSection = .automatic
    @State
    private var commitTask: Task<Void, Never>?
    /// The picked pressing, awaiting the footer's Set identity.
    @State
    private var selectedPressing: Pressing?
    private enum Phase: Equatable {
        case identifying
        case committing
        case error(DisplayError)
    }

    private var key: String {
        ReIdentifyKey.make(forReleaseId: releaseId)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
        }
        .frame(width: 700, height: 600)
        .background(Theme.background)
        .task {
            await startReIdentify()
        }
        .onDisappear {
            commitTask?.cancel()
            // End this release's identification session in core.
            let importer = importer
            let uiStore = uiStore
            let key = key
            Task {
                do { try await importer.endReleaseIdentification(key) }
                catch { uiStore.showError(error) }
            }
            // Drop the candidate so a re-open starts fresh.
            importStore.reIdentifyCandidates.removeValue(forKey: key)
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text("Re-identify")
                    .themeText(.heading)
                Text(displayName)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Button("Close") { onClose() }
                .keyboardShortcut(.cancelAction)
        }
        .padding()
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        switch phase {
        case .identifying:
            if let candidate = importStore.reIdentifyCandidates[key] {
                CandidateRuntimeReader(key: key) { runtime in
                    CandidateSignalsReader(key: key) { signals in
                        identifyPane(
                            candidate: candidate,
                            runtime: runtime,
                            signals: signals
                        )
                    }
                }
            }
            else {
                ProgressView("Starting re-identify...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        case .committing:
            ProgressView("Committing identity...")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .error(let failure):
            errorBanner(failure)
        }
    }

    /// The identify pane, reading its state and signals under this sheet's
    /// key.
    private func identifyPane(
        candidate: Candidate,
        runtime: BridgeCandidateRuntimeSnapshot?,
        signals: Signals?
    ) -> some View {
        VStack(spacing: 0) {
            ImportSearchFlow.buildSearchPane(
                services: ImportSearchFlow.ImportServices(
                    importer: importer,
                    importStore: importStore
                ),
                input: ImportSearchFlow.SearchPaneInput(
                    candidate: candidate,
                    key: key,
                    selectedReleaseId: selectedPressing?.lead.releaseId,
                    runtime: runtime,
                    openSection: openSection,
                    onOpenSection: { openSection = $0 },
                    // A library release has no folder draft to keep.
                    onKeepOwnDraft: nil,
                    // Nor a folder to link to an album.
                    onLinkSharedAlbum: nil,
                    // Its runs start straight away, never waiting on the
                    // identification queue.
                    onCancelIdentification: nil,
                    liveSignals: signals
                ),
                openSettings: {
                    settingsNavigation.open(
                        .importing,
                        present: { openSettings() }
                    )
                },
                // The sheet's header closes it.
                onBack: nil,
                // No confirm page: the footer commits the picked pressing.
                // Opening a row asks core to read its release, as a pick
                // does on the import surface.
                onSelect: { pressing in
                    selectedPressing = pressing
                    importer.openSearchResult(key, pressing.link)
                }
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            if let selectedPressing {
                selectionFooter(for: selectedPressing)
            }
        }
    }

    // MARK: - Selection footer

    /// Footer that commits the picked pressing as the release's identity.
    private func selectionFooter(
        for pressing: Pressing
    ) -> some View {
        HStack(alignment: .center, spacing: ThemeSpace.group) {
            Spacer(minLength: 0)
            Button("Set identity") {
                commit(pressing.link)
            }
            .buttonStyle(PrimaryButtonStyle())
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
        .background(Theme.surface)
        .overlay(alignment: .top) {
            Rectangle().fill(Theme.hairline).frame(height: 1)
        }
    }

    // MARK: - Error banner

    private func errorBanner(_ failure: DisplayError) -> some View {
        VStack(spacing: ThemeSpace.group) {
            Image(systemName: "exclamationmark.triangle.fill")
                .themeIcon(.hero)
                .foregroundStyle(Theme.danger)
            Text("Re-identify failed.")
                .themeText(.heading)
            ErrorDetailDisclosure(
                error: failure,
                tone: .neutral,
                showIcon: false
            )
            .frame(maxWidth: Self.messageWidth)
            Button("Close") { onClose() }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Actions

extension ReIdentifySheet {
    fileprivate func startReIdentify() async {
        // Seed the candidate the search pane reads; identify state comes from
        // the candidate-runtime signal under the same key.
        if importStore.reIdentifyCandidates[key] == nil {
            importStore.reIdentifyCandidates[key] = Candidate(
                reIdentifyKey: key,
                releaseId: releaseId,
                displayName: displayName
            )
        }
        importer.autoIdentifyRelease(key, releaseId)
    }

    /// Apply the picked pressing to the release's edit session, then close
    /// this sheet and open the editor on what the apply filled in. Saving
    /// there writes it all; cancelling drops the session.
    fileprivate func commit(_ link: BridgePressingLink) {
        commitTask?.cancel()
        phase = .committing
        let releaseEditor = releaseEditor
        let importer = importer
        let uiStore = uiStore
        let releaseId = self.releaseId
        let onClose = self.onClose
        commitTask = Task { @MainActor in
            do {
                let seed = try await releaseEditor.applyReleaseEditCatalog(
                    releaseId,
                    link
                )
                onClose()
                uiStore.presentModal {
                    EditMetadataSheet(
                        releaseId: releaseId,
                        seed: seed,
                        commands: releaseEditor.editCommands(
                            searchAlbums: importer.searchAlbums
                        ),
                        onSaved: { albumId in
                            uiStore.dismissModal()
                            uiStore.navigateToAlbum(
                                albumId,
                                releaseId: releaseId
                            )
                        },
                        onViewAlbum: { albumId in
                            uiStore.dismissModal()
                            uiStore.navigateToAlbum(albumId)
                        },
                        onCancel: {
                            uiStore.dismissModal()
                            Task {
                                try? await releaseEditor.discardReleaseEdit(
                                    releaseId
                                )
                            }
                        },
                    )
                }
            }
            catch is CancellationError {
                logger.debug(
                    "Re-identify apply cancelled for release \(releaseId)"
                )
            }
            catch {
                logger.error(
                    "Re-identify apply failed: \(error.localizedDescription)"
                )
                // A failure with nothing to show gets no error phase.
                if let failure = DisplayError(error) {
                    phase = .error(failure)
                }
            }
        }
    }
}

#if DEBUG
    // The stub importer never identifies, so the pane stays in its start state.
    #Preview("Re-identify Sheet") {
        ReIdentifySheet(
            releaseId: "rel-a-01",
            displayName: "Album Title \u{00B7} 2019 \u{00B7} CD",
            onClose: {},
        )
        .environment(SettingsNavigation())
        .albumDetailPreviewEnvironment(store: PreviewData.seededLibraryStore())
        .candidateReaderPreviewEnvironment()
        .mainWindowPreviewEnvironment()
        .preferredColorScheme(.dark)
    }
#endif
