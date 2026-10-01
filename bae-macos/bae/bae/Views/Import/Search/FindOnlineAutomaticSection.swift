import BaeKit
import SwiftUI

/// The AUTOMATIC section's content: the run's band of identifiers with what it
/// matched beneath, scrolling together — or, with nothing to lay out, one line
/// saying so and the one thing to do.
///
/// Which of those it is, is `FindOnlineResultArea`'s answer, read off the
/// identify state. Everything the section shows hangs off that one reading,
/// so the whole of it lives here rather than in the pane that stacks the two
/// section headers.
struct FindOnlineAutomaticSection: View {
    let state: ImportSearchState
    /// Open Settings on the Discogs page — offered when no source is on.
    let onOpenSettings: () -> Void
    /// Turn one identifier in the band over — the disc ID, a barcode, a
    /// catalog number. Core re-derives the state the import projection
    /// delivers from what the candidate's choices then say.
    let onToggleLookup: (LookupToggle) -> Void
    /// Strike a catalog number in the Catalog # row out of the run, or put it
    /// back: a struck number is neither searched nor agreed with, so either
    /// starts another run.
    let onToggleCatalogAgreement: (String) -> Void
    /// Re-ask only the lookups that failed, keeping what the others found.
    let onRetryFailed: () -> Void
    /// Search by the words the person left in the title chip.
    let onEditTitleSearch: (_ album: String, _ artist: String) -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (Pressing) -> Void
    /// Hand the pane over to SEARCH with the cursor in its first field.
    let onSearchManually: () -> Void
    /// Take the candidate off the identification queue and go back to the
    /// draft; `nil` where the surface's candidate is never queued.
    let onCancelIdentification: (() -> Void)?
    /// Keep the folder's own draft over what the lookup offered; `nil` where
    /// the surface has no draft of its own to keep.
    let onKeepOwnDraft: (() -> Void)?
    /// Link the folder to the album the offered pressings are of, its
    /// pressing unknown; `nil` where the surface has no folder to link.
    let onLinkSharedAlbum: (() -> Void)?
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
        configStore.config.lookupCatalogs.contains { $0.availability == .on }
    }

    var body: some View {
        switch area {
        case .notStarted:
            FindOnlineEmptyZone {
                if hasSourceToSearch {
                    // Nothing has run for this candidate. Starting one is the
                    // card's own action, so what is left here is the other way
                    // to a release: asking for it by name.
                    SearchManuallyButton(action: onSearchManually)
                }
                else {
                    Text("No source to search")
                        .foregroundStyle(.secondary)
                    Button("Open Settings", action: onOpenSettings)
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                }
            }
        case .queued:
            FindOnlineEmptyZone {
                Text("Queued to be identified")
                    .foregroundStyle(.secondary)
                if let onCancelIdentification {
                    Button("Cancel", action: onCancelIdentification)
                        .controlSize(.small)
                }
            }
        case .noSignals:
            FindOnlineEmptyZone {
                if let needsYou = state.needsYou {
                    Text(verbatim: needsYou.sentence)
                        .multilineTextAlignment(.center)
                }
                else {
                    Text("No disc ID, barcode, or catalog number found")
                        .foregroundStyle(.secondary)
                }
                SearchManuallyButton(action: onSearchManually)
                keepOwnDraft
            }
        case .error(let failure):
            FindOnlineEmptyZone {
                Text(failure.detail)
                    .foregroundStyle(Theme.warning)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
                Button("Retry", action: onRetryFailed)
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
            }
        case .identifying, .groups, .nothingFound, .awaitingCatalog,
            .failureLines:
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if let sentence = headerSentence {
                        NeedsYouSentence(text: sentence)
                        Divider()
                            .padding(.horizontal, ThemeSpace.group)
                    }
                    if let run = state.run {
                        IdentifierBand(
                            run: run,
                            onToggleLookup: onToggleLookup,
                            onToggleCatalogAgreement:
                                onToggleCatalogAgreement,
                            onRetryFailed: onRetryFailed,
                            onEditTitleSearch: onEditTitleSearch
                        )
                        Divider()
                            .padding(.horizontal, ThemeSpace.group)
                    }
                    belowBand
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    @ViewBuilder
    private var belowBand: some View {
        switch area {
        case .identifying:
            if !state.identifiedGroups.isEmpty {
                identifiedList { narrowedOut }
            }
        case .groups:
            identifiedList {
                // The run picked a release and could not apply it: why, with
                // the retry, since the band has no failed cell to offer one.
                if !pickFailures.isEmpty {
                    FindOnlineFailureLines(
                        failures: pickFailures,
                        onRetry: onRetryFailed
                    )
                }
                // A folder waiting on the person heads the page with why.
                if state.needsYou == nil,
                    let folderCheck = state.folderCheck?.localizedText
                {
                    FolderCheckNote(text: folderCheck)
                }
                // Right under the offered rows, above the ones agreement set
                // aside: the answer for a person who cannot tell them apart.
                linkSharedAlbum
                    .padding(.leading, ReleaseGroupSection.rowTextInset)
                narrowedOut
                ForEach(missingSourceNotes, id: \.search) { note in
                    MissingSourceNote(text: note.text)
                }
                keepOwnDraft
                    .padding(.leading, ReleaseGroupSection.rowTextInset)
            }
        case .nothingFound:
            FindOnlineEmptyZone {
                if let needsYou = state.needsYou {
                    Text(verbatim: needsYou.sentence)
                        .multilineTextAlignment(.center)
                }
                else {
                    Text("No results")
                        .foregroundStyle(.secondary)
                }
                SearchManuallyButton(action: onSearchManually)
                keepOwnDraft
            }
        case .failureLines:
            if !state.failureLines.isEmpty {
                // No cell shows these, so none offers their retry: the lines
                // do.
                FindOnlineFailureLines(
                    failures: state.failureLines,
                    onRetry: onRetryFailed
                )
            }
        case .awaitingCatalog:
            FindOnlineEmptyZone { keepOwnDraft }
        case .notStarted, .queued, .noSignals, .error:
            EmptyView()
        }
    }

    /// The way to answer a folder waiting on the person with its own draft,
    /// worded for whether the page offered releases to pick from.
    @ViewBuilder
    private var keepOwnDraft: some View {
        if let needsYou = state.needsYou, let onKeepOwnDraft {
            PaneAnswerAction(
                title: needsYou.offeredReleases
                    ? String(localized: "None of these")
                    : String(localized: "Keep my info"),
                subtitle: String(
                    localized: "Keep my info; it's not in the catalogs"
                ),
                action: onKeepOwnDraft
            )
            .disabled(state.isImporting)
        }
    }

    /// The way to answer a folder whose copy is one of several pressings of
    /// one album, without saying which: offered where core says the rows
    /// are that.
    @ViewBuilder
    private var linkSharedAlbum: some View {
        if state.offersSharedAlbum, let onLinkSharedAlbum {
            PaneAnswerAction(
                title: String(localized: "Not sure"),
                subtitle: String(
                    localized:
                        "Use what these releases share; leave the rest as is"
                ),
                action: onLinkSharedAlbum
            )
            .disabled(state.isImporting)
        }
    }

    /// The sentence heading a page whose folder waits on the person, where
    /// the page lists what was found; an empty page says it in its middle.
    private var headerSentence: String? {
        switch area {
        case .groups, .awaitingCatalog: state.needsYou?.sentence
        case .identifying, .nothingFound, .failureLines, .notStarted,
            .queued, .noSignals, .error:
            nil
        }
    }

    /// Whether the rows the signals agreed away show on their cards, under
    /// every card and above what the list says about itself. Nothing narrowed,
    /// nothing to disclose.
    @ViewBuilder
    private var narrowedOut: some View {
        if state.narrowedOutCount > 0 {
            NarrowedOutDisclosure(
                count: state.narrowedOutCount,
                isExpanded: $narrowedOutExpanded
            )
        }
    }

    private func identifiedList<Trailing: View>(
        @ViewBuilder trailing: @escaping () -> Trailing
    ) -> some View {
        ReleaseGroupListContent(
            groups: state.identifiedGroups,
            showsNarrowedOut: narrowedOutExpanded,
            isImporting: state.isImporting,
            libraryStatuses: state.libraryStatuses,
            agreements: state.identifiedAgreements,
            selectedReleaseId: state.selectedReleaseId
                ?? state.finalizingPressing?.lead.releaseId,
            loadingReleaseId: state.loadingReleaseId
                ?? state.finalizingPressing?.lead.releaseId,
            releaseSelectionFailure: state.releaseSelectionFailure,
            onRetryUnread: onRetryFailed,
            onSelect: onSelect,
            trailing: trailing,
        )
    }

    /// The failed fetches of the release the run picked, which leave no
    /// results missing from the list: they are why the pick was not applied.
    private var pickFailures: [BridgeIdentifyFailure] {
        state.failureLines.filter { $0.failedSearch == nil }
    }

    /// One line per failed lookup whose results the list is missing and no
    /// cell of the band shows, closing it. Named by step as well as source:
    /// the source's other steps may have answered, and those results are on
    /// the list.
    private var missingSourceNotes: [(search: FailedSearch, text: String)] {
        var seen: Set<FailedSearch> = []
        return state.failureLines.compactMap { failure in
            guard let search = failure.failedSearch,
                seen.insert(search).inserted
            else { return nil }
            let source = bridgeCatalogName(catalog: search.source)
            let step = SignalBadgeStyle.sentenceLabel(for: search.step)
            return (
                search: search,
                text: String(
                    localized:
                        "\(source) \(step) results are missing from this list."
                )
            )
        }
    }
}

/// A way to answer the folder other than picking a row: the button, and
/// what it does beneath it.
private struct PaneAnswerAction: View {
    let title: String
    let subtitle: String
    let action: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
            Button(title, action: action)
                .controlSize(.small)
            Text(subtitle)
                .themeText(.detail)
                .foregroundStyle(.tertiary)
        }
    }
}

/// What happened to a folder that waits on the person, in one plain sentence
/// over everything the page lists.
private struct NeedsYouSentence: View {
    let text: String

    var body: some View {
        Text(verbatim: text)
            .themeText(.body)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, ThemeSpace.group)
            .padding(.vertical, ThemeSpace.related)
    }
}

/// Why the release found was not picked for the folder: the check against the
/// folder it failed, under its row.
private struct FolderCheckNote: View {
    let text: String

    var body: some View {
        Text(text)
            .themeText(.detail)
            .foregroundStyle(Theme.warning)
            .padding(.leading, ReleaseGroupSection.rowTextInset)
    }
}
