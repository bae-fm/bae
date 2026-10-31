import BaeKit
import SwiftUI

/// The one release editor: a library release's cover, fields, album and
/// tracks, with the catalog search that applies a release onto them, laid
/// out as the import pane lays out a folder's draft. The library's Edit…
/// sheet and the import pane of an imported folder both show it.
struct ReleaseEditorView: View {
    let session: ReleaseMetadataEditSession
    /// What the catalog search names the release by.
    let displayName: String
    /// Show a library album the album row names.
    let onViewAlbum: (String) -> Void
    var onPlayTrack: ((String) -> Void)?
    /// The save landed; receives the album the release is on after it.
    let onSaved: @MainActor @Sendable (String) -> Void

    /// The draft, or the catalog search opened on one of its sections.
    @State
    private var searchSection: BridgeFindOnlineSection?
    @State
    private var showingCoverPicker = false

    @Environment(Importer.self)
    private var importer
    @Environment(ImportStore.self)
    private var importStore
    @Environment(ReleaseEditor.self)
    private var releaseEditor
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.openSettingsWindow)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation

    /// The key core runs the release's catalog search under.
    private var searchKey: Data {
        bridgeReidentifyKey(releaseId: session.releaseId)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            actionRow
            if let shown = session.validationMessage.map({
                DisplayError(line: $0)
            })
                ?? session.failure
            {
                ErrorDetailDisclosure(error: shown)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            if let searchSection {
                search(searchSection)
            }
            else {
                ReleaseMetadataEditorContent(
                    session: session,
                    onEditCover: { showingCoverPicker = true },
                    onViewAlbum: onViewAlbum,
                    onPlayTrack: onPlayTrack
                )
            }
        }
        .sheet(isPresented: $showingCoverPicker) { coverPicker }
        .onDisappear { endSearch() }
    }

    /// The identify entries the import pane offers, then the session's own
    /// Cancel and Save.
    private var actionRow: some View {
        HStack(alignment: .center, spacing: ThemeSpace.edge) {
            HStack(spacing: ThemeSpace.related) {
                Text("Identify")
                    .themeText(.strong)
                    .foregroundStyle(.secondary)
                Button("Automatic") { identifyAutomatically() }
                    .buttonStyle(.bordered)
                Button("Search") { openSearch(.search) }
                    .buttonStyle(.bordered)
            }
            .disabled(session.isBusy)
            Spacer(minLength: ThemeSpace.group)
            ProgressView()
                .controlSize(.small)
                .opacity(session.isBusy ? 1 : 0)
            Button("Cancel") {
                searchSection = nil
                session.cancelChanges()
            }
            .opacity(session.hasChanges ? 1 : 0)
            .allowsHitTesting(session.hasChanges)
            .disabled(session.isBusy)
            Button("Save") {
                session.save(onSuccess: onSaved)
            }
            .buttonStyle(PrimaryButtonStyle())
            .keyboardShortcut(.defaultAction)
            .disabled(session.isBusy)
        }
    }

    /// The catalog search under this release's key; picking a pressing
    /// applies it to the session at once and goes back to the draft.
    @ViewBuilder
    private func search(_ section: BridgeFindOnlineSection) -> some View {
        if let candidate = importStore.reIdentifyCandidates[searchKey] {
            CandidateRuntimeReader(key: searchKey) { runtime in
                CandidateSignalsReader(key: searchKey) { signals in
                    ImportSearchFlow.buildSearchPane(
                        services: ImportSearchFlow.ImportServices(
                            importer: importer,
                            importStore: importStore
                        ),
                        input: ImportSearchFlow.SearchPaneInput(
                            candidate: candidate,
                            key: searchKey,
                            selectedReleaseId: nil,
                            runtime: runtime,
                            openSection: section,
                            onOpenSection: { searchSection = $0 },
                            // A library release has no folder draft to keep.
                            onKeepOwnDraft: nil,
                            // Nor a folder to link to an album.
                            onLinkSharedAlbum: nil,
                            // Its runs start straight away, never waiting on
                            // the identification queue.
                            onCancelIdentification: nil,
                            liveSignals: signals
                        ),
                        openSettings: {
                            settingsNavigation.open(
                                .importing,
                                present: { openSettings() }
                            )
                        },
                        onBack: { searchSection = nil },
                        onSelect: { pressing in
                            searchSection = nil
                            session.apply(pressing.link)
                        }
                    )
                }
            }
            .frame(maxWidth: .infinity)
            .card()
        }
    }

    private var coverPicker: some View {
        CoverSheetView(
            releaseId: session.releaseId,
            onFindRelease: {
                showingCoverPicker = false
                openSearch(.search)
            },
            fetchRemoteCovers: {
                try await releaseEditor.fetchRemoteCovers(
                    .release(releaseId: session.releaseId)
                )
            },
            onSelect: { selection in
                try await releaseEditor.changeCover(
                    session.releaseId,
                    selection
                )
                try await session.refreshCover()
            },
            onDone: { showingCoverPicker = false }
        )
        .frame(width: 1_000, height: 740)
    }

    /// Open the search and start identifying the release from its files.
    private func identifyAutomatically() {
        openSearch(.automatic)
        importer.autoIdentifyRelease(session.releaseId)
    }

    /// Open the search on `section`, starting nothing.
    private func openSearch(_ section: BridgeFindOnlineSection) {
        if importStore.reIdentifyCandidates[searchKey] == nil {
            importStore.reIdentifyCandidates[searchKey] = Candidate(
                reIdentifyKey: searchKey,
                releaseId: session.releaseId,
                displayName: displayName
            )
        }
        searchSection = section
    }

    /// End the release's identification in core and forget its search, so
    /// the editor opened again starts fresh.
    private func endSearch() {
        guard importStore.reIdentifyCandidates[searchKey] != nil else {
            return
        }
        importStore.reIdentifyCandidates.removeValue(forKey: searchKey)
        let importer = importer
        let uiStore = uiStore
        let releaseId = session.releaseId
        Task {
            do { try await importer.endReleaseIdentification(releaseId) }
            catch { uiStore.showError(error) }
        }
    }
}
