import BaeKit
import SwiftUI

/// The catalog search under a library release's key: picking a pressing
/// applies it to the session at once and goes back to the draft.
struct ReleaseEditorCatalogSearch: View {
    let session: ReleaseMetadataEditSession
    /// The section the search is open on.
    let section: FindOnlineSection
    /// The key core runs the release's catalog search under.
    let searchKey: Data
    let onOpenSection: (FindOnlineSection) -> Void
    let onBack: () -> Void

    @Environment(Importer.self)
    private var importer
    @Environment(ImportStore.self)
    private var importStore
    @Environment(\.openSettingsWindow)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation

    var body: some View {
        if let candidate = importStore.reIdentifyCandidates[searchKey] {
            CandidateRuntimeReader(key: searchKey) { runtime in
                ImportSearchFlow.buildSearchPane(
                    services: ImportSearchFlow.ImportServices(
                        importer: importer,
                        importStore: importStore
                    ),
                    input: ImportSearchFlow.SearchPaneInput(
                        candidate: candidate,
                        key: searchKey,
                        runtime: runtime,
                        openSection: section,
                        onOpenSection: onOpenSection,
                        // A library release has no folder draft to keep.
                        onKeepOwnDraft: nil,
                        // Nor a folder to link to an album.
                        onLinkSharedAlbum: nil,
                        // Its runs start straight away, never waiting on
                        // the identification queue.
                        onCancelIdentification: nil
                    ),
                    openSettings: {
                        settingsNavigation.open(
                            .importing,
                            present: { openSettings() }
                        )
                    },
                    onBack: onBack,
                    onSelect: { pressing in
                        onBack()
                        session.apply(pressing.link)
                    }
                )
            }
            .frame(maxWidth: .infinity)
            .card()
        }
    }
}
