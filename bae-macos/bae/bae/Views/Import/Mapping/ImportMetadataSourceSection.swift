import BaeKit
import SwiftUI

/// The metadata slot: the candidate's one editable draft, or the Find online
/// page a person opens to identify it.
struct ImportMetadataSourceSection: View {
    let candidate: Candidate
    /// Whether the candidate can be edited and identified now; its catalog
    /// links stay live either way.
    let actionable: Bool
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    let editActions: ReleaseFieldWriter
    let albumActions: DraftAlbumActions
    let editingCommands: EditingCommitCommands
    let endEditing: @MainActor () async -> Void
    let commit: ImportCommitControls?
    /// Move the pane as the person asked.
    let onMovePane: (PaneMove) -> Void
    /// Show identification's results for this candidate, as core decides.
    let onIdentify: () -> Void
    /// Open the pane on its typed search, starting nothing.
    let onSearchForRelease: () -> Void
    let onResetToFileMetadata: () -> Void
    let onClearMetadata: () -> Void
    let onUnlink: () -> Void
    let onEditCover: () -> Void
    let onDropCoverImage: (String) -> Void

    var body: some View {
        Group {
            switch candidate.metadataPresentation {
            case .draft:
                draft
            case .findOnline:
                ImportOnlineMetadataBrowser(
                    candidateKey: candidate.key,
                    endEditing: endEditing,
                    onMovePane: onMovePane
                )
                .disabled(!actionable)
            }
        }
    }

    @ViewBuilder
    private var draft: some View {
        if let edit = candidate.edit,
            let offers = candidate.draft?.pressingOffers
        {
            MacImportDraft(
                releaseSummary: ImportReleaseSummary(
                    candidate: candidate,
                    editValues: edit
                ),
                actionable: actionable,
                isReading: isReading,
                coverContent: coverContent,
                hasCoverOptions: hasCoverOptions,
                editValues: edit,
                pressingOffers: offers,
                albumArtistStanding: candidate.draft?.albumArtistStanding,
                records: candidate.records,
                releaseLink: candidate.releaseLink,
                // A blank draft files under nothing yet.
                album: candidate.metadataDraftIsBlank
                    ? nil : candidate.draft?.album,
                albumActions: albumActions,
                editActions: editActions,
                editingCommands: editingCommands,
                commit: commit,
                sourceActions: ImportReleaseSourceActions(
                    identifyAutomatically: onIdentify,
                    searchForRelease: onSearchForRelease,
                    resetToFileMetadata: onResetToFileMetadata,
                    clearMetadata: onClearMetadata,
                    unlink: onUnlink
                ),
                onEditCover: onEditCover,
                onDropCoverImage: onDropCoverImage
            )
        }
        else {
            // The candidate's draft is a local read; nothing shows until it
            // lands.
            Color.clear
                .frame(maxWidth: .infinity, minHeight: 180)
        }
    }
}

/// Find online for the candidate, read from the store that its form and picks
/// write, with what is in flight for it: the run whose state it shows while
/// one is live, and the search's answers.
private struct ImportOnlineMetadataBrowser: View {
    let candidateKey: Data
    let endEditing: @MainActor () async -> Void
    let onMovePane: (PaneMove) -> Void

    @Environment(Importer.self)
    private var importer
    @Environment(ImportStore.self)
    private var importStore
    @Environment(\.openSettingsWindow)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation

    var body: some View {
        if let candidate = importStore.candidate(forKey: candidateKey) {
            CandidateRuntimeReader(key: candidateKey) { runtime in
                ImportSearchFlow.buildSearchPane(
                    services: ImportSearchFlow.ImportServices(
                        importer: importer,
                        importStore: importStore
                    ),
                    input: ImportSearchFlow.SearchPaneInput(
                        candidate: candidate,
                        key: candidateKey,
                        runtime: runtime,
                        openSection: candidate.panePosition.findOnlineSection,
                        onOpenSection: {
                            onMovePane(.openSection($0))
                        },
                        onKeepOwnDraft: keepOwnDraft,
                        onLinkSharedAlbum: linkSharedAlbum,
                        onCancelIdentification: cancelIdentification
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

    /// Keep the candidate's own draft over what its lookup offered, and go
    /// back to the draft once core holds it. Its failure is stated on the
    /// pane, from what core stored.
    private func keepOwnDraft() {
        Task { @MainActor in
            await endEditing()
            do {
                if try await importer.keepCandidateDraft(candidateKey) == .done
                {
                    onMovePane(.picked)
                }
            }
            catch is CancellationError {}
            catch { importStore.reportFailure(error) }
        }
    }

    /// Take the candidate off the identification queue, and go back to the
    /// draft once it is off, as Back does.
    private func cancelIdentification() {
        Task { @MainActor in
            do {
                try await importer.cancelCandidateIdentification(candidateKey)
                onMovePane(.back)
            }
            catch is CancellationError {}
            catch { importStore.reportFailure(error) }
        }
    }

    /// Link the candidate to the album the offered pressings are of, its
    /// pressing unknown, and go back to the draft once core holds it. Its
    /// failure is stated on the pane, from what core stored.
    private func linkSharedAlbum() {
        Task { @MainActor in
            await endEditing()
            do {
                if try await importer.linkCandidateSharedAlbum(candidateKey)
                    == .done
                {
                    onMovePane(.picked)
                }
            }
            catch is CancellationError {}
            catch { importStore.reportFailure(error) }
        }
    }
}
