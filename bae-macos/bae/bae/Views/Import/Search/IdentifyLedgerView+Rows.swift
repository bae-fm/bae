import BaeKit
import SwiftUI

// The pieces a ledger is built from: a signal's group label, a value's row
// with its source chips and provider cells, and the cell itself.

/// A signal's name at the top of its group. A spinner beside it means the
/// producing scan may still add rows to the group; a signal with nothing
/// found never disappears — its label dims and carries a short dash.
struct LedgerGroupLabel: View {
    let text: String
    var working = false
    /// Nothing to run for this signal.
    var nothing = false
    /// Reading this signal's input failed before any provider was asked.
    var failure: BridgeLookupFailure?

    var body: some View {
        HStack(spacing: 7) {
            Text(text)
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(
                    nothing ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.primary)
                )
                .fixedSize()
            if working {
                ProgressView()
                    .controlSize(.small)
                    .scaleEffect(0.55)
                    .frame(width: 10, height: 10)
            }
            if nothing {
                LedgerDash()
                    .padding(.leading, 1)
            }
            if failure != nil {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, LedgerMetrics.sideInset)
        .frame(height: working ? 30 : 26)
    }
}

/// The short dash that says nothing ran here.
struct LedgerDash: View {
    var body: some View {
        RoundedRectangle(cornerRadius: 1)
            .fill(Color.primary.opacity(0.28))
            .frame(width: 8, height: 1.5)
    }
}

/// A full-width band connecting a value to its status cells.
struct LedgerRowBand<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        HStack(spacing: 0) {
            content
        }
        .padding(.leading, LedgerMetrics.rowInset)
        .padding(.trailing, LedgerMetrics.sideInset)
        .frame(height: 28)
        .background(Color.primary.opacity(0.022))
        .padding(.bottom, 2)
    }
}

/// The value itself, in monospace.
struct LedgerValueText: View {
    let value: String

    var body: some View {
        Text(value)
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .truncationMode(.middle)
    }
}

/// One value's row under the provider table: where it was found, the value,
/// and one cell per provider.
struct LedgerValueRow: View {
    let row: BridgeSignalValueRow
    let filePaths: [String: String]
    let onRetry: () -> Void

    var body: some View {
        LedgerRowBand {
            HStack(spacing: 7) {
                ForEach(Array(row.sources.enumerated()), id: \.offset) {
                    _,
                    source in
                    SignalSourceChip(source: source, filePaths: filePaths)
                }
                LedgerValueText(value: row.value)
            }
            Spacer(minLength: 8)
            ForEach(row.cells, id: \.source) { cell in
                LookupCellView(lookup: cell.lookup, onRetry: onRetry)
                    .frame(width: LedgerMetrics.cellWidth)
            }
        }
    }
}

/// One provider's lookup of one value, as a glyph: spinner looking up, green
/// count matched, gray 0 answered empty, a small dot queued, a dash never
/// needed, a warning with its own Retry failed.
struct LookupCellView: View {
    let lookup: BridgeLookupState
    let onRetry: () -> Void

    var body: some View {
        switch lookup {
        case .queued:
            Circle()
                .fill(Color.primary.opacity(0.18))
                .frame(width: 5, height: 5)
        case .notAsked:
            LedgerDash()
        case .lookingUp:
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.6)
                .frame(width: 11, height: 11)
        case .found(let count, let groups):
            LookupCountView(count: Int(count), groups: groups)
        case .noMatch:
            CountCapsule(count: 0)
        case .failed(let failure):
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
                    .help(failure.badgeLine)
                Button(action: onRetry) {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(Color.accentColor)
                }
                .buttonStyle(.plain)
                .help("Retry")
            }
        }
    }
}

/// A match count, with the releases it stands for a hover away: year,
/// label, catalog number, region and format, sectioned by album when the
/// lookup spans several.
struct LookupCountView: View {
    let count: Int
    let groups: [BridgeReleaseGroup]

    var body: some View {
        CountCapsule(count: count)
            .hoverPopover(arrowEdge: .bottom) {
                LookupReleasesPopover(
                    groups: groups.map(ReleaseGroup.init(bridge:))
                )
                .popoverEntrance(anchor: .top)
                .background { PopoverBehavior() }
            }
    }
}

/// The releases one lookup found. One album lists its pressings alone;
/// several list each album's cover, title and artist, then its pressings.
struct LookupReleasesPopover: View {
    let groups: [ReleaseGroup]

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            if groups.count == 1, let group = groups.first {
                ForEach(group.pressings) { pressing in
                    LookupReleaseLine(pressing: pressing)
                }
            }
            else {
                ForEach(Array(groups.enumerated()), id: \.element.id) {
                    index,
                    group in
                    if index > 0 {
                        Rectangle()
                            .fill(Theme.hairline)
                            .frame(height: 1)
                            .padding(.horizontal, 2)
                            .padding(.vertical, 4)
                    }
                    HStack(spacing: 6) {
                        ImageView(
                            content: group.coverImageContent,
                            pointSize: 16
                        )
                        .frame(width: 16, height: 16)
                        .clipShape(RoundedRectangle(cornerRadius: 3))
                        Text(group.title)
                            .font(.system(size: 11, weight: .semibold))
                            .lineLimit(1)
                        if let artist = group.artist {
                            Text(verbatim: "\u{00b7} \(artist)")
                                .font(.system(size: 11))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                        }
                    }
                    .padding(.horizontal, 6)
                    .padding(.top, 4)
                    .padding(.bottom, 2)
                    ForEach(group.pressings) { pressing in
                        LookupReleaseLine(pressing: pressing)
                            .padding(.leading, 22)
                    }
                }
            }
        }
        .padding(6)
        .frame(width: 264)
    }
}

/// One pressing a lookup found: year, label, catalog number, region and
/// format — the facts that tell pressings of one album apart.
struct LookupReleaseLine: View {
    let pressing: Pressing

    private var pressed: String {
        [pressing.lead.country, pressing.lead.format]
            .compactMap { $0 }
            .joined(separator: " \u{00b7} ")
    }

    var body: some View {
        HStack(spacing: 7) {
            if let year = pressing.lead.year {
                Text(String(year))
                    .font(.system(size: 11.5, weight: .semibold))
                    .monospacedDigit()
            }
            if let label = pressing.lead.label {
                Text(label)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if let catalogNumber = pressing.lead.catalogNumber {
                Text(catalogNumber)
                    .font(.system(size: 9.5, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 4)
                    .background(
                        Theme.hover,
                        in: RoundedRectangle(cornerRadius: 3)
                    )
                    .lineLimit(1)
            }
            if !pressed.isEmpty {
                Text(pressed)
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Lookup releases — one album") {
        LookupReleasesPopover(groups: [PreviewData.searchGroupExact])
            .importPreviewEnvironment()
    }

    #Preview("Lookup releases — several albums") {
        LookupReleasesPopover(groups: PreviewData.searchGroupsManual)
            .importPreviewEnvironment()
    }
#endif
