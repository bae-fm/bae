import BaeKit
import SwiftUI

/// Find online: one page. The header states what identification concluded and
/// offers the one thing to do about it, the result area lists what there is to
/// pick from, and the typed-search form stays docked at the bottom.
///
/// A submitted search takes over the result area only — the verdict above it
/// does not move, and Clear gives the area back. Renders from
/// `ImportSearchState` plus the form bindings and action callbacks.
struct ImportSearchPane: View {
    let state: ImportSearchState
    /// Leave the pane. `nil` for a surface that owns its own way out.
    let onBack: (() -> Void)?
    /// The typed-search form as the candidate stores it.
    let form: CandidateSearchState
    /// The form as the person left it, to store with the candidate.
    let onCommitForm: (CandidateSearchState) -> Void
    /// Search with the form as it stands.
    let onSearch: (CandidateSearchState) -> Void
    /// Drop the submitted search, giving the result area back to identify.
    let onClearSearch: () -> Void
    /// Re-ask only the providers whose part of the search failed.
    let onRetrySearch: () -> Void
    let onOpenSettings: () -> Void
    /// Act on a signal — take the disc ID or barcode in or out of the run, or
    /// pick which extracted catalog number the run looks up. The state the
    /// import projection delivers is re-derived from what is left checked.
    let onToggleSignal: (BridgeSignalToggle) -> Void
    /// Start identification for a folder whose run never began. Core owns
    /// whether this starts, resumes, or does nothing.
    let onIdentify: () -> Void
    /// Run signal extraction and the lookups again.
    let onRerun: () -> Void
    /// Re-ask only the lookups that failed, keeping what the others found.
    let onRetryFailed: () -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (Pressing) -> Void
    let onSourceSearch: (ReleaseGroup, BridgeMetadataSource) -> Void

    /// The form's first field takes the keyboard on every new value. A
    /// person with nothing to pick is going to type, so an empty result area
    /// hands the cursor over, and "Search instead" hands it over again.
    @State
    private var formFocusRequest = 0

    private var verdict: FindOnlineVerdict {
        FindOnlineVerdict(
            state: state.identifyState,
            toolbar: state.signalsToolbar
        )
    }

    private var area: FindOnlineResultArea {
        FindOnlineResultArea(
            identifyState: state.identifyState,
            hasSearch: state.search != nil
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            FindOnlineHeader(
                verdict: verdict,
                toolbar: state.signalsToolbar,
                onBack: onBack,
                onIdentify: onIdentify,
                onRetry: onRetryFailed,
                onToggleSignal: onToggleSignal,
                onRerun: onRerun,
            )
            Divider()
            errorLine
            resultArea
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            Divider()
            ImportSearchFormView(
                form: form,
                onCommit: onCommitForm,
                signals: state.signals,
                focusRequest: formFocusRequest,
                onSearch: onSearch,
            )
        }
        // A person looking at an empty result area is about to type: seed the
        // Artist field from what was read off the folder and put the cursor
        // in it. Only when the fields are untouched — never over typing.
        .onChange(of: area, initial: true) { _, area in
            guard area == .nothingFound || area == .noSignals else { return }
            formFocusRequest += 1
            guard form.searchArtist.isEmpty, form.searchAlbum.isEmpty else {
                return
            }
            if let seed = state.signals?.text.freeText.first {
                var seeded = form
                seeded.searchArtist = seed
                onCommitForm(seeded)
            }
        }
    }

    @ViewBuilder
    private var errorLine: some View {
        if let error = state.error {
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                Text(error)
                Spacer()
            }
            .font(.caption)
            .foregroundStyle(.red)
            .padding(.horizontal, 14)
            .padding(.vertical, 6)
        }
    }

    // MARK: - Result area

    @ViewBuilder
    private var resultArea: some View {
        switch area {
        case .identifying:
            identifying
        case .groups:
            ReleaseGroupListView(
                groups: state.identifiedGroups,
                isImporting: state.isImporting,
                libraryStatuses: state.libraryStatuses,
                provenance: state.identifiedProvenance,
                selectedReleaseId: state.selectedReleaseId,
                loadingReleaseId: state.loadingReleaseId,
                releaseSelectionFailure: state.releaseSelectionFailure,
                onSelect: onSelect,
                onSourceSearch: onSourceSearch,
                trailing: {
                    ForEach(missingSourceNotes, id: \.self) { note in
                        MissingSourceNote(text: note)
                    }
                    finalizingLine
                },
            )
        case .nothingFound:
            FindOnlineEmptyZone {
                Text("No matches.")
                    .foregroundStyle(.secondary)
                searchInstead
            }
        case .noSignals:
            FindOnlineEmptyZone {
                Text("Nothing to identify.")
                    .foregroundStyle(.secondary)
                searchInstead
            }
        case .notStarted:
            FindOnlineEmptyZone {
                IdentifyAutomaticallyButton(action: onIdentify)
            }
        case .failureLines:
            failureLines
        case .searchRun:
            searchRun
        }
    }

    /// The way out of an empty zone: the cursor goes to the form's first
    /// field. Every press is a new request, so it works after the cursor has
    /// been elsewhere.
    private var searchInstead: some View {
        Button("Search instead") { formFocusRequest += 1 }
            .buttonStyle(.link)
    }

    /// Every lookup failed, so the reasons take the place of the results.
    private var failureLines: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(state.identifyFailures, id: \.badgeLine) { failure in
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text(failure.badgeLine)
                    Spacer(minLength: 0)
                }
            }
        }
        .font(.system(size: 12.5))
        .padding(.horizontal, 18)
        .padding(.vertical, 22)
        .frame(
            maxWidth: .infinity,
            maxHeight: .infinity,
            alignment: .topLeading
        )
    }

    /// One line per failed lookup whose results the list is missing, closing
    /// it. Named by step as well as source: the source's other steps may have
    /// answered, and those results are on the list.
    private var missingSourceNotes: [String] {
        var seen: Set<FailedSearch> = []
        return state.identifyFailures.compactMap { failure in
            guard let search = failure.failedSearch,
                seen.insert(search).inserted
            else { return nil }
            let source = bridgeMetadataSourceName(source: search.source)
            let step = SignalBadgeStyle.sentenceLabel(for: search.step)
            return String(
                localized:
                    "\(source) \(step) results are missing from this list."
            )
        }
    }

    /// What core is still doing after the verdict: a sole pressing has its
    /// details fetched and applied as the pick, then the answer is stored.
    /// The list is already final, so the line sits under it rather than
    /// replacing it.
    @ViewBuilder
    private var finalizingLine: some View {
        if state.isFinalizing {
            let pressings = state.identifiedGroups.flatMap(\.pressings).count
            HStack(spacing: 6) {
                ProgressView()
                    .controlSize(.small)
                    .scaleEffect(0.7)
                Text(
                    pressings == 1
                        ? String(localized: "Fetching release details\u{2026}")
                        : String(localized: "Saving the result\u{2026}")
                )
            }
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .padding(.leading, 28)
        }
    }

    /// The run as its steps, each provider's part settling on its own, with
    /// whatever the answered lookups have combined to listed beneath. The
    /// list scrolls under the steps, which stay put, so a person keeps the
    /// run in view while the matches come in.
    @ViewBuilder
    private var identifying: some View {
        if case .triangulating(let run, _, _, _) = state.identifyState {
            VStack(alignment: .leading, spacing: 0) {
                IdentifyRunStepsView(
                    run: run,
                    catalogOptions: state.signalsToolbar.signals
                        .first { $0.kind == .catalog }?
                        .options ?? [],
                    onToggleSignal: onToggleSignal,
                    onRetryFailed: onRetryFailed,
                )
                if !state.identifiedGroups.isEmpty {
                    Divider()
                    ReleaseGroupListView(
                        groups: state.identifiedGroups,
                        isImporting: state.isImporting,
                        libraryStatuses: state.libraryStatuses,
                        provenance: state.identifiedProvenance,
                        selectedReleaseId: state.selectedReleaseId,
                        loadingReleaseId: state.loadingReleaseId,
                        releaseSelectionFailure: state.releaseSelectionFailure,
                        onSelect: onSelect,
                        onSourceSearch: onSourceSearch,
                        trailing: { EmptyView() },
                    )
                }
            }
        }
    }

    @ViewBuilder
    private var searchRun: some View {
        if let search = state.search {
            FindOnlineSearchResults(
                search: search,
                isImporting: state.isImporting,
                libraryStatuses: state.libraryStatuses,
                selectedReleaseId: state.selectedReleaseId,
                loadingReleaseId: state.loadingReleaseId,
                releaseSelectionFailure: state.releaseSelectionFailure,
                onClear: onClearSearch,
                onRetry: onRetrySearch,
                onOpenSettings: onOpenSettings,
                onSelect: onSelect,
                onSourceSearch: onSourceSearch,
            )
        }
    }
}

#if DEBUG
    // MARK: - Previews

    extension ImportSearchPane {
        /// Preview builder — fixes the form bindings and action callbacks to
        /// inert defaults so a preview states only the situation it exercises.
        @MainActor
        static func preview(
            state: ImportSearchState,
            searchArtist: String = "",
            searchAlbum: String = "",
        ) -> ImportSearchPane {
            ImportSearchPane(
                state: state,
                onBack: {},
                form: CandidateSearchState(
                    searchArtist: searchArtist,
                    searchAlbum: searchAlbum
                ),
                onCommitForm: { _ in },
                onSearch: { _ in },
                onClearSearch: {},
                onRetrySearch: {},
                onOpenSettings: {},
                onToggleSignal: { _ in },
                onIdentify: {},
                onRerun: {},
                onRetryFailed: {},
                onSelect: { _ in },
                onSourceSearch: { _, _ in },
            )
        }
    }

    #Preview("Find online — identifying") {
        ImportSearchPane.preview(state: PreviewData.searchStateTriangulating)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — found, cross-linked") {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — signals named different releases") {
        ImportSearchPane.preview(state: PreviewData.searchStateDisagreement)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — nothing found") {
        ImportSearchPane.preview(state: PreviewData.searchStateNotFound)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — no signals") {
        ImportSearchPane.preview(state: PreviewData.searchStateNoSignals)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — source failure, partial results") {
        ImportSearchPane.preview(state: PreviewData.searchStateSourceFailure)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — every source failed") {
        ImportSearchPane.preview(state: PreviewData.searchStateAllSourcesFailed)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — not identified") {
        ImportSearchPane.preview(state: PreviewData.searchStateIdle)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — searching") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateSearching,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title One",
        )
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
    }

    #Preview("Find online — search results") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateManual,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title One",
        )
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
    }

    #Preview("Find online — a searched source dropped") {
        ImportSearchPane.preview(state: PreviewData.searchStateSearchFailed)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — search matched nothing") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateSearchEmpty,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title",
        )
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
    }
#endif
