import BaeKit
import SwiftUI

/// One pressing under a release-group card: its facts, the signals that named
/// it and the sources that list it; one already in the library is dimmed.
struct ImportSearchResultRow: View {
    /// The inset from the row's box to its text.
    static let horizontalPadding = ThemeSpace.related

    let pressing: Pressing
    let isImporting: Bool
    let libraryStatus: BridgeLibraryStatus?
    /// What the candidate's own text agrees with about this pressing; `nil`
    /// for typed-search results.
    var agreements: BridgeAgreements?
    let isSelected: Bool
    /// The pick being read on this row, while one is: the row names the
    /// network read it waits on.
    var loading: LoadingPick?
    var failure: ReleaseSelectionFailure?
    /// Identify the candidate again, reading once more the documents a run
    /// could not; `nil` where no run read any, as for a typed search.
    var onRetryUnread: (() -> Void)?
    let onSelect: (Pressing) -> Void

    private var isInLibrary: Bool {
        libraryStatus?.releaseInLibrary == true
    }

    /// Whether the row can be picked: no import is running and its pick is not
    /// already loading.
    var isPickable: Bool {
        !isImporting && loading == nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            resultButton
            StepProgressLine(steps: followedSteps) { EmptyView() }
                .padding(.horizontal, Self.horizontalPadding)
            unattendedLine
            failureLine
        }
        .background(rowBackground)
    }

    private var resultButton: some View {
        ZStack {
            Button {
                onSelect(pressing)
            } label: {
                Rectangle()
                    .fill(.clear)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!isPickable)

            HStack(spacing: ThemeSpace.related) {
                facts
                    .opacity(isInLibrary ? 0.55 : 1)
                    .allowsHitTesting(false)
                signalBadges
                Spacer(minLength: ThemeSpace.related)
                libraryMarker
                    .allowsHitTesting(false)
                sourceTags
                    .allowsHitTesting(false)
                chevron
                    .allowsHitTesting(false)
            }
        }
        .padding(.vertical, ThemeSpace.compact)
        .padding(.horizontal, Self.horizontalPadding)
    }

    /// The steps of the person's pick being read on this row.
    private var followedSteps: LatestSource<BridgeCatalogApplyStep>? {
        guard case .followed(let steps) = loading?.reading else { return nil }
        return steps
    }

    private var isReadingUnattended: Bool {
        if case .unattended = loading?.reading { return true }
        return false
    }

    /// The run's own pick being read in full on this row, kept in the
    /// layout with no height while there is none.
    private var unattendedLine: some View {
        Text(
            "Reading the release in full…",
            comment:
                "Find online, under the pressing identification picked on its own: bae reads the release from its catalogs before it stores the answer."
        )
        .themeText(.body)
        .foregroundStyle(.secondary)
        .padding(.horizontal, Self.horizontalPadding)
        .padding(.bottom, ThemeSpace.related)
        .frame(height: isReadingUnattended ? nil : 0, alignment: .top)
        .clipped()
        .opacity(isReadingUnattended ? 1 : 0)
        .accessibilityHidden(!isReadingUnattended)
    }

    /// The row's failure and its retry: picking it again when loading the pick
    /// failed, or identifying again when a document could not be read.
    var rowFailure: (error: DisplayError, retry: () -> Void)? {
        if let failure, failure.matches(pressing) {
            return (failure.error, { onSelect(pressing) })
        }
        if let unread = pressing.documentFailure, let onRetryUnread {
            let line = BridgeIdentifyFailure.releaseDetails(failure: unread)
                .badgeLine
            return (DisplayError(line: line), onRetryUnread)
        }
        return nil
    }

    @ViewBuilder
    private var failureLine: some View {
        if let failure = rowFailure {
            Notice(
                error: failure.error,
                action: ErrorAction(
                    isDisabled: !isPickable,
                    perform: failure.retry
                )
            )
            .themeText(.body)
            .padding(.horizontal, Self.horizontalPadding)
            .padding(.bottom, ThemeSpace.related)
        }
    }

    private var rowBackground: some View {
        RoundedRectangle(cornerRadius: ThemeRadius.control)
            .fill(
                isSelected || rowFailure != nil
                    ? Theme.accentSoft : .clear
            )
            .overlay(
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .strokeBorder(
                        isSelected || rowFailure != nil
                            ? Theme.accentStrong : .clear,
                        lineWidth: 1
                    )
            )
    }

    // MARK: - What the pressing is

    private var facts: some View {
        HStack(spacing: ThemeSpace.related) {
            if let year = pressing.lead.year {
                Text(String(year))
                    .themeText(.strong)
                    .monospacedDigit()
            }
            else {
                Text("Year unknown")
                    .themeText(.body)
                    .foregroundStyle(.tertiary)
            }
            ForEach(Array(pressing.labels.enumerated()), id: \.offset) {
                _,
                label in
                if !label.names.isEmpty {
                    Text(
                        label.names.joined(
                            separator: localizedCoreString(
                                "core.label.list_separator"
                            )
                        )
                    )
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                }
                ForEach(
                    Array(label.catalogNumbers.enumerated()),
                    id: \.offset
                ) { _, catalogNumber in
                    StatusChip(verbatim: catalogNumber)
                }
            }
            let summary = pressing.summaryText
            if !summary.isEmpty {
                Text(summary)
                    .themeText(.detail)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            let details = pressing.detailsText
            if !details.isEmpty {
                Text(details)
                    .themeText(.detail)
                    .foregroundStyle(.quaternary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }

    // MARK: - Agreement badges

    /// Badges for what the candidate's own text agrees with about this row, in
    /// one fixed order.
    @ViewBuilder
    private var signalBadges: some View {
        if let agreements {
            HStack(spacing: ThemeSpace.inline) {
                agreementBadge(.discId, on: agreements.discId)
                agreementBadge(.barcode, on: agreements.barcode)
                agreementBadge(.catalog, on: agreements.catalog)
                agreementBadge(.label, on: agreements.label)
                agreementBadge(.year, on: agreements.year)
                agreementBadge(.country, on: agreements.country)
                if let note = agreements.notes {
                    // Takes the pointer so its note shows on hover; a click
                    // on it picks the row like a click anywhere else on it.
                    agreementChip(.notes)
                        .help(note)
                        .onTapGesture {
                            if isPickable {
                                onSelect(pressing)
                            }
                        }
                }
            }
        }
    }

    /// A badge only for an agreement the row has, so the badges pack without
    /// gaps.
    @ViewBuilder
    private func agreementBadge(
        _ agreement: SignalBadgeStyle.Agreement,
        on: Bool
    ) -> some View {
        if on {
            agreementChip(agreement)
                .allowsHitTesting(false)
        }
    }

    private func agreementChip(_ agreement: SignalBadgeStyle.Agreement)
        -> some View
    {
        StatusChip(
            verbatim: SignalBadgeStyle.label(for: agreement),
            tone: .accent
        )
        // A badge is one word; the row's pressing text truncates instead.
        .fixedSize()
    }

    // MARK: - Trailing

    /// Every source listing this pressing, in core's order; picking the row
    /// takes all of them.
    private var sourceTags: some View {
        HStack(spacing: ThemeSpace.inline) {
            ForEach(Array(pressing.sources.enumerated()), id: \.element) {
                at,
                source in
                if at > 0 {
                    Text(verbatim: "\u{00b7}")
                        .themeText(.detail)
                        .foregroundStyle(.quaternary)
                }
                Text(bridgeLookupCatalogName(catalog: source))
                    .themeText(.detail)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    /// The "In library" tag, hidden by opacity so the column keeps its width.
    private var libraryMarker: some View {
        HStack(spacing: ThemeSpace.inline) {
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(Theme.success)
            Text("In library")
        }
        .themeText(.detail)
        .foregroundStyle(.tertiary)
        .opacity(isInLibrary ? 1 : 0)
        .accessibilityHidden(!isInLibrary)
    }

    private var chevron: some View {
        Image(systemName: "chevron.right")
            .themeIcon(.small)
            .foregroundStyle(
                isSelected ? Theme.accent : Theme.hairlineStrong
            )
            .frame(width: ThemeIcon.small.size)
    }
}

#if DEBUG
    // MARK: - Preview

    #Preview("Pressing rows") {
        VStack(spacing: ThemeSpace.hairline) {
            ImportSearchResultRow(
                pressing: PreviewData.searchGroupExact.pressings[0],
                isImporting: false,
                libraryStatus: nil,
                agreements: BridgeAgreements(
                    discId: true,
                    barcode: false,
                    catalog: true,
                    label: true,
                    year: true,
                    country: false,
                    notes: "Small label"
                ),
                isSelected: true,
                onSelect: { _ in },
            )
            ImportSearchResultRow(
                pressing: PreviewData.searchGroupsManual[1].pressings[0],
                isImporting: false,
                libraryStatus: nil,
                agreements: nil,
                isSelected: false,
                loading: LoadingPick(
                    releaseId: PreviewData.searchGroupsManual[1].pressings[0]
                        .lead.releaseId,
                    reading: .followed(
                        LatestStream.catalogApplySteps(
                            holding: .readingRelease(catalog: .discogs)
                        )
                        .only
                    )
                ),
                onSelect: { _ in },
            )
            ImportSearchResultRow(
                pressing: PreviewData.searchGroupExact.pressings[1],
                isImporting: false,
                libraryStatus: BridgeLibraryStatus(
                    releaseId: "rel-456",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albums: [
                        BridgeLibraryAlbumMatch(
                            albumId: "album-1",
                            albumTitle: "Album Title",
                            holdsRelease: true
                        )
                    ],
                ),
                agreements: nil,
                isSelected: false,
                onSelect: { _ in },
            )
        }
        .padding()
        .frame(width: 620)
        .importPreviewEnvironment()
    }
#endif
