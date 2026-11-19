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
    /// Open settings on the Discogs page — offered when no source is on.
    let onOpenSettings: () -> Void
    /// Turn one identifier in the band over — the Disc ID, a barcode, a
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

    /// Whether any source is asked is core's answer, carried on the config the
    /// app observes: adding a token in Settings takes the notice away while
    /// the pane is open. With none, there is nothing to start: core refuses to
    /// switch off the last source, so this is reachable only by a source
    /// losing its credential after being left as the only one switched on.
    @Environment(ConfigStore.self)
    private var configStore

    private var area: FindOnlineResultArea {
        FindOnlineResultArea(identifyState: state.identifyState)
    }

    var body: some View {
        switch area {
        case .unread:
            EmptyView()
        case .notStarted:
            FindOnlineEmptyZone {
                if configStore.config.hasSourceToSearch {
                    // Nothing has run for this candidate. Starting one is the
                    // card's own action, so what is left here is the other way
                    // to a release: asking for it by name.
                    SearchManuallyButton(action: onSearchManually)
                }
                else {
                    Text("No source to search")
                        .foregroundStyle(.secondary)
                    MacButton(
                        "Open settings",
                        kind: .primary,
                        action: onOpenSettings
                    )
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
                    Text("No Disc ID, barcode, or catalog number found")
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
                MacButton("Retry", kind: .primary, action: onRetryFailed)
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
                        MacIdentifierBand(
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
                if let failures = state.failureLines?.pickFailures {
                    FindOnlineFailureLines(
                        failures: failures,
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
                    .padding(.leading, ThemeSpace.group)
                narrowedOut
                ForEach(
                    state.failureLines?.missingSearches ?? [],
                    id: \.self
                ) {
                    MissingSourceNote(text: Self.missingText($0))
                }
                keepOwnDraft
                    .padding(.leading, ThemeSpace.group)
            }
        case .nothingFound:
            FindOnlineEmptyZone {
                Text("No results")
                    .foregroundStyle(.secondary)
                SearchManuallyButton(action: onSearchManually)
                keepOwnDraft
            }
        case .failureLines:
            if let lines = state.failureLines {
                // No cell shows these, so none offers their retry: the lines
                // do.
                FindOnlineFailureLines(
                    failures: lines.all,
                    onRetry: onRetryFailed
                )
            }
        case .awaitingCatalog:
            FindOnlineEmptyZone { keepOwnDraft }
        case .notStarted, .queued, .noSignals, .error, .unread:
            EmptyView()
        }
    }

    /// The way to answer a folder waiting on the person with its own draft,
    /// worded for whether the page offered releases to pick from.
    @ViewBuilder
    private var keepOwnDraft: some View {
        if let needsYou = state.needsYou, let onKeepOwnDraft {
            MacAnswerAction(
                title: Self.keepOwnDraftTitle(needsYou),
                subtitle: String(
                    localized: "Keep my info; it is not in the catalogs"
                ),
                off: state.isImporting,
                action: onKeepOwnDraft
            )
        }
    }

    /// The words of the way to keep the folder's own draft: none of these
    /// where the page offered releases, keep mine where it offered none.
    static func keepOwnDraftTitle(_ needsYou: BridgeNeedsYouReason) -> String {
        needsYou.offeredReleases
            ? String(localized: "None of these")
            : String(localized: "Keep my info")
    }

    /// The way to answer a folder whose copy is one of several pressings of
    /// one album, without saying which: offered where core says the rows
    /// are that.
    @ViewBuilder
    private var linkSharedAlbum: some View {
        if state.offersSharedAlbum, let onLinkSharedAlbum {
            MacAnswerAction(
                title: String(localized: "Not sure"),
                subtitle: String(
                    localized:
                        "Use what these releases share; leave the rest as is"
                ),
                off: state.isImporting,
                action: onLinkSharedAlbum
            )
        }
    }

    /// The sentence heading a page whose folder waits on the person, where
    /// the page lists what was found or found nothing.
    private var headerSentence: String? {
        switch area {
        case .groups, .awaitingCatalog, .nothingFound:
            state.needsYou?.sentence
        case .identifying, .failureLines, .notStarted,
            .queued, .noSignals, .error, .unread:
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
            agreements: state.identifiedAgreements,
            loadingPick: state.loadingPick,
            releaseSelectionFailure: state.releaseSelectionFailure,
            onRetryUnread: onRetryFailed,
            onSelect: onSelect,
            trailing: trailing,
        )
    }

    /// The line closing the list for a failed lookup whose results the list
    /// is missing. The line names the step as well as the source, because
    /// the source's other steps may have answered, and those results are on
    /// the list.
    static func missingText(_ search: BridgeMissingSearch) -> String {
        let source = bridgeCatalogName(catalog: search.source)
        let step = SignalBadgeStyle.sentenceLabel(for: search.step)
        return String(
            localized: "\(source) \(step) results are missing from this list."
        )
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
            .padding(.leading, MacReleaseGroup.rowTextInset)
    }
}
