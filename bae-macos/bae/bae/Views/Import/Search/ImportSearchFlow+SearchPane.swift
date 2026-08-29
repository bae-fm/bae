import BaeKit
import SwiftUI

extension ImportSearchFlow {
    // MARK: - Shared search pane builder

    /// The import-flow services a search pane drives: search/identify and the
    /// pick command on `importer`, candidate state on `importStore`, and
    /// Discogs availability on `configStore`.
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

    /// `onSelect` defaults to the import flow's prefetch + docked-pane path
    /// (picking a pressing opens the confirm pane). Re-identify overrides it
    /// to select the pressing and commit `re_identify_release` from its own
    /// footer, since the release is already in the library — there's no second
    /// editable confirmation page.
    @MainActor
    @ViewBuilder
    static func buildSearchPane(
        services: ImportServices,
        input: SearchPaneInput,
        mode: Binding<SearchMode>,
        openSettings: @escaping () -> Void,
        onUseFileTags: (() -> Void)? = nil,
        onSelect: ((BridgeMetadataResult) -> Void)? = nil
    ) -> some View {
        let key = input.key
        let importStore = services.importStore
        let resolvedOnSelect =
            onSelect ?? defaultOnSelect(services: services, input: input)
        let fields = searchFieldBindings(importStore: importStore, input: input)

        ImportSearchPane(
            state: searchPaneState(
                candidate: input.candidate,
                services: services,
                input: input
            ),
            mode: mode,
            activeTab: fields.activeTab,
            activeSource: fields.activeSource,
            searchArtist: fields.artist,
            searchAlbum: fields.album,
            searchCatalog: fields.catalog,
            searchBarcode: fields.barcode,
            onSearch: {
                dispatchSearch(
                    importer: services.importer,
                    importStore: importStore,
                    key: key
                )
            },
            onOpenSettings: openSettings,
            onUseFileTags: onUseFileTags,
            onToggleSignal: { signal in
                services.importer.toggleSignalForCandidate(key, signal)
            },
            onIdentify: {
                services.importer.identifyForExplicitLookup(key)
            },
            onRerun: { services.importer.rerunIdentifyForCandidate(key) },
            onSelect: resolvedOnSelect,
        )
    }

    /// The tab/source/text bindings the pane's form edits, each writing back
    /// through `mutateCandidate` so edits land on the candidate in the store.
    struct SearchFieldBindings {
        let activeTab: Binding<SearchTab>
        let activeSource: Binding<BridgeMetadataSource>
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
            activeSource: makeActiveSourceBinding(
                importStore: importStore,
                key: key,
                candidate: candidate
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
        let tabResults = candidate.search.activeResults()
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

    /// The default row-pick handler: decide the identity, which persists the
    /// choice and comes back with what it claims. Re-identify overrides this
    /// with its own `onSelect`.
    @MainActor
    private static func defaultOnSelect(
        services: ImportServices,
        input: SearchPaneInput
    ) -> (BridgeMetadataResult) -> Void {
        { result in
            applyMetadata(
                importer: services.importer,
                importStore: services.importStore,
                key: input.key,
                provenance: .externalRelease(
                    source: result.source,
                    releaseId: result.releaseId
                )
            )
        }
    }
}
