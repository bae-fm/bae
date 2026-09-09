import BaeKit
import SwiftUI

/// The AUTOMATIC section's content: the identify ledger with what it matched
/// beneath, scrolling together — or, with nothing to lay out, one line saying
/// so and the one thing to do.
///
/// Which of those it is, is `FindOnlineResultArea`'s answer, read off the
/// identify state. Everything the section shows hangs off that one reading,
/// so the whole of it lives here rather than in the pane that stacks the two
/// section headers.
struct FindOnlineAutomaticSection: View {
    let state: ImportSearchState
    /// Open Settings on the Discogs page — offered when no source is on.
    let onOpenSettings: () -> Void
    /// Take a catalog number in or out of the run. Core re-derives the state
    /// the import projection delivers from what is chosen.
    let onToggleCatalog: (String) -> Void
    /// Count a catalog number the folder states, or stop counting it. Nothing
    /// is looked up: the answers in hand are ranked by the new value the next
    /// time the candidate is read.
    let onToggleCatalogAgreement: (String) -> Void
    /// Start identification for a folder whose run never began. Core owns
    /// whether this starts, resumes, or does nothing.
    let onIdentify: () -> Void
    /// Re-ask only the lookups that failed, keeping what the others found.
    let onRetryFailed: () -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (Pressing) -> Void
    /// Hand the pane over to SEARCH with the cursor in its first field.
    let onSearchManually: () -> Void
    /// Whether the releases agreement narrowed out are showing. Held by the
    /// pane, which outlives this section: collapsing AUTOMATIC and opening it
    /// again leaves the disclosure as the person left it.
    @Binding
    var narrowedOutExpanded: Bool

    /// Which sources are asked is core's answer, carried on the config the app
    /// observes: adding a token in Settings takes the notice away while the
    /// pane is open.
    @Environment(ConfigStore.self)
    private var configStore

    private var area: FindOnlineResultArea {
        FindOnlineResultArea(identifyState: state.identifyState)
    }

    /// Whether any source is being asked at all. With none, there is nothing to
    /// start: core refuses to switch off the last source, so this is reachable
    /// only by a source losing its credential after being left as the only one
    /// switched on.
    private var hasSourceToSearch: Bool {
        configStore.config.metadataSources.contains { $0.availability == .on }
    }

    var body: some View {
        switch area {
        case .notStarted:
            FindOnlineEmptyZone {
                if hasSourceToSearch {
                    IdentifyButton(action: onIdentify)
                }
                else {
                    Text("No source to search")
                        .foregroundStyle(.secondary)
                    Button("Open Settings", action: onOpenSettings)
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                }
            }
        case .noSignals:
            FindOnlineEmptyZone {
                Text("No disc ID, barcode, or catalog number found")
                    .foregroundStyle(.secondary)
                SearchManuallyButton(action: onSearchManually)
            }
        case .identifying, .groups, .nothingFound, .awaitingCatalog,
            .failureLines:
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if let run = state.run {
                        IdentifyLedgerView(
                            run: run,
                            catalogAgreements: state.catalogAgreements,
                            filePaths: state.filePaths,
                            onToggleCatalog: onToggleCatalog,
                            onToggleCatalogAgreement:
                                onToggleCatalogAgreement,
                            onRetryFailed: onRetryFailed
                        )
                        Divider()
                            .padding(.horizontal, 14)
                    }
                    belowLedger
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    @ViewBuilder
    private var belowLedger: some View {
        switch area {
        case .identifying:
            if !state.identifiedGroups.isEmpty {
                identifiedList { narrowedOut }
            }
        case .groups:
            identifiedList {
                narrowedOut
                ForEach(missingSourceNotes, id: \.self) { note in
                    MissingSourceNote(text: note)
                }
            }
        case .nothingFound:
            FindOnlineEmptyZone {
                Text("No results")
                    .foregroundStyle(.secondary)
                SearchManuallyButton(action: onSearchManually)
            }
        case .failureLines:
            failureLines
        case .awaitingCatalog, .notStarted, .noSignals:
            EmptyView()
        }
    }

    /// What the signals agreed away, under the matches and above what the
    /// list says about itself. Nothing narrowed, nothing to disclose.
    @ViewBuilder
    private var narrowedOut: some View {
        if !state.narrowedOut.isEmpty {
            NarrowedOutDisclosure(
                narrowedOut: state.narrowedOut,
                isExpanded: $narrowedOutExpanded,
                isImporting: state.isImporting,
                selectedReleaseId: state.selectedReleaseId,
                loadingReleaseId: state.loadingReleaseId,
                releaseSelectionFailure: state.releaseSelectionFailure,
                onSelect: onSelect,
            )
        }
    }

    private func identifiedList<Trailing: View>(
        @ViewBuilder trailing: @escaping () -> Trailing
    ) -> some View {
        ReleaseGroupListContent(
            groups: state.identifiedGroups,
            isImporting: state.isImporting,
            libraryStatuses: state.libraryStatuses,
            agreements: state.identifiedAgreements,
            selectedReleaseId: state.selectedReleaseId
                ?? state.finalizingPressing?.lead.releaseId,
            loadingReleaseId: state.loadingReleaseId
                ?? state.finalizingPressing?.lead.releaseId,
            releaseSelectionFailure: state.releaseSelectionFailure,
            onSelect: onSelect,
            trailing: trailing,
        )
    }

    /// The reasons, with the retry they carry when no ledger does.
    private var failureLines: some View {
        FindOnlineFailureLines(
            failures: state.identifyFailures,
            onRetry: state.run == nil ? onRetryFailed : nil
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
}
