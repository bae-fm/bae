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
    /// Whether this row's pick is loading, shown by the row's own spinner.
    var isLoading: Bool = false
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
        !isImporting && !isLoading
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            resultButton
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

    /// The row's failure and its retry: picking it again when loading the pick
    /// failed, or identifying again when a document could not be read.
    private var rowFailure: (error: DisplayError, retry: () -> Void)? {
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

    private var failureLine: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            ErrorDetailDisclosure(error: rowFailure?.error)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: ThemeSpace.related)
            Button("Retry") { rowFailure?.retry() }
                .buttonStyle(.link)
                .disabled(!isPickable)
        }
        .themeText(.body)
        .padding(.horizontal, Self.horizontalPadding)
        .padding(.bottom, ThemeSpace.related)
        .frame(height: rowFailure == nil ? 0 : nil, alignment: .top)
        .clipped()
        .opacity(rowFailure == nil ? 0 : 1)
        .allowsHitTesting(rowFailure != nil)
        .accessibilityHidden(rowFailure == nil)
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
                            separator: QueueSummary.message(
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
                Text(bridgeCatalogName(catalog: source))
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

    /// The spinner and chevron swap by opacity, so a pick starting to load
    /// does not re-measure the other rows.
    private var chevron: some View {
        ZStack {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.7)
                .opacity(isLoading ? 1 : 0)
            Image(systemName: "chevron.right")
                .themeIcon(.small)
                .foregroundStyle(
                    isSelected ? Theme.accent : Theme.hairlineStrong
                )
                .opacity(isLoading ? 0 : 1)
        }
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
                isLoading: true,
                onSelect: { _ in },
            )
            ImportSearchResultRow(
                pressing: PreviewData.searchGroupExact.pressings[1],
                isImporting: false,
                libraryStatus: BridgeLibraryStatus(
                    releaseId: "rel-456",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: "album-1",
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
