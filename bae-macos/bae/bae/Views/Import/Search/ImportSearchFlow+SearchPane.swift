import BaeKit
import SwiftUI

extension ImportSearchFlow {
    // MARK: - Shared search pane builder

    /// The import-flow services a search pane drives: search and identify on
    /// `importer`, candidate state on `importStore`, and Discogs availability
    /// on `configStore`. The opening surface owns what selecting a result does.
    struct ImportServices {
        let importer: Importer
        let importStore: ImportStore
        let configStore: ConfigStore
    }

    /// Which candidate a search pane renders, and the selection state it shows:
    /// `selectedReleaseId` is the pressing whose confirm pane is open, so its
    /// row renders selected.
    struct SearchPaneInput {
        let candidate: Candidate
        let key: String
        let selectedReleaseId: String?
        /// What is in flight for this key: the run whose state and badge row
        /// the pane shows. `nil` when nothing is running for it.
        let runtime: BridgeCandidateRuntimeSnapshot?
        /// What extraction has found for this key so far, feeding the manual
        /// form's suggestion pools and its scanning indicator. `nil` before
        /// extraction has reported any, and for a candidate whose run settled
        /// in an earlier session — the stored row answers for that one.
        let liveSignals: Signals?
    }

    /// `onSelect` owns what picking a pressing means for the surface that
    /// opened the pane. Import applies it to the candidate draft; re-identify
    /// keeps it selected until its own footer commits the library release.
    @MainActor
    @ViewBuilder
    static func buildSearchPane(
        services: ImportServices,
        input: SearchPaneInput,
        mode: Binding<BridgeDefaultFindOnlineMode>,
        openSettings: @escaping () -> Void,
        onUseFileTags: (() -> Void)? = nil,
        onSelect: @escaping (BridgeMetadataResult) -> Void
    ) -> some View {
        let key = input.key
        let importStore = services.importStore
        let fields = searchFieldBindings(importStore: importStore, input: input)

        ImportSearchPane(
            state: searchPaneState(
                candidate: input.candidate,
                services: services,
                input: input
            ),
            mode: mode,
            activeTab: fields.activeTab,
            musicBrainzSelected: fields.musicBrainzSelected,
            discogsSelected: fields.discogsSelected,
            searchArtist: fields.artist,
            searchAlbum: fields.album,
            searchCatalog: fields.catalog,
            searchBarcode: fields.barcode,
            onSearch: {
                dispatchSearch(
                    importer: services.importer,
                    importStore: importStore,
                    key: key,
                    discogsAvailable: services.configStore.config.discogsUsable
                )
            },
            onOpenSettings: openSettings,
            onUseFileTags: onUseFileTags,
            onToggleSignal: { signal in
                services.importer.toggleSignalForCandidate(key, signal)
            },
            onEnterAutomatic: {
                services.importer.identifyForExplicitLookup(key)
            },
            onRerun: { services.importer.rerunIdentifyForCandidate(key) },
            onSelect: onSelect,
        )
    }

    /// The tab/source/text bindings the pane's form edits, each writing back
    /// through `mutateCandidate` so edits land on the candidate in the store.
    struct SearchFieldBindings {
        let activeTab: Binding<SearchTab>
        let musicBrainzSelected: Binding<Bool>
        let discogsSelected: Binding<Bool>
        let artist: Binding<String>
        let album: Binding<String>
        let catalog: Binding<String>
        let barcode: Binding<String>
    }

    @MainActor
    private static func searchFieldBindings(
        importStore: ImportStore,
        input: SearchPaneInput
    ) -> SearchFieldBindings {
        let candidate = input.candidate
        let key = input.key
        func text(
            _ field: WritableKeyPath<CandidateSearchState, String>
        ) -> Binding<String> {
            makeSearchFieldBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: field
            )
        }
        return SearchFieldBindings(
            activeTab: makeActiveTabBinding(
                importStore: importStore,
                key: key,
                candidate: candidate
            ),
            musicBrainzSelected: makeSearchSourceBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: \.musicBrainzSelected
            ),
            discogsSelected: makeSearchSourceBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: \.discogsSelected
            ),
            artist: text(\.searchArtist),
            album: text(\.searchAlbum),
            catalog: text(\.searchCatalog),
            barcode: text(\.searchBarcode)
        )
    }

    /// The pane's read-only state snapshot from the candidate, plus the
    /// open-confirm selection and Discogs availability the pane renders against.
    @MainActor
    private static func searchPaneState(
        candidate: Candidate,
        services: ImportServices,
        input: SearchPaneInput
    ) -> ImportSearchState {
        let tabResults = candidate.search.activeResults(
            discogsAvailable: services.configStore.config.discogsUsable
        )
        return ImportSearchState(
            identifyState: shownIdentifyState(
                resumed: candidate.resumedIdentifyState,
                runtime: input.runtime
            ),
            error: candidate.error,
            searchGroups: tabResults.groups,
            selectedReleaseId: input.selectedReleaseId,
            loadingReleaseId: candidate.loadingReleaseId,
            isSearching: tabResults.isSearching,
            hasSearched: tabResults.hasSearched,
            isImporting: isImporting(candidate),
            libraryStatuses: candidate.libraryStatuses,
            discogsEnabled: services.configStore.config.discogsUsable,
            // The run in flight knows more than the last stored answer does,
            // and for a re-identify key — which has no row at all — it is the
            // only answer.
            signals: input.liveSignals ?? candidate.settledSignals,
            signalsToolbar: input.runtime?.signalsToolbar
                ?? BridgeSignalsToolbar(signals: []),
        )
    }

}
