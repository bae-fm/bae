import BaeKit
import SwiftUI

/// Find online for the candidate, read from the store that its form and picks
/// write, with what is in flight for it: the run whose state it shows while
/// one is live, and the search's answers.
struct ImportOnlineMetadataBrowser: View {
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
