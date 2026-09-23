import BaeKit
import SwiftUI

// The pieces the band is built from: one identifier's chip, a provider's
// answer inside it, and the marks a chip carries when there is no answer to
// show.

/// Where a chip's value was read, as tags between its label and the value.
enum IdentifierTags {
    /// Every place a barcode or catalog number was read, in the order it was
    /// read there. Empty for a chip with no value yet.
    case sources([BridgeValueSource])
    /// The LOG or CUE a disc ID was read off. `nil` for a release
    /// re-identified from its stored tracks, which has no file to name.
    case discIdFile(BridgeDiscIdFile?)
}

/// How a chip reads: filled for an identifier the run has an answer about,
/// outlined and dimmed for a number waiting to be looked up.
enum IdentifierChipStyle {
    case filled
    case outlined
}

/// The widest a chip's value is drawn before it truncates in the middle: a
/// disc ID is far longer than a barcode, and the band reads as chips rather
/// than as one long line.
private let identifierValueWidth: CGFloat = 92

/// One identifier in the band: what kind it is, where it was read, the value,
/// and what the providers say about it.
struct IdentifierChip<Trailing: View>: View {
    let label: String
    var tags: IdentifierTags = .sources([])
    var value: String?
    var style: IdentifierChipStyle = .filled
    @ViewBuilder
    let trailing: Trailing

    @State
    private var isHovered = false

    var body: some View {
        HStack(spacing: 6) {
            IdentifierLabel(text: label)
            tagChips
            if let value {
                Text(value)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(valueStyle)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: identifierValueWidth, alignment: .leading)
            }
            trailing
        }
        .padding(.leading, 7)
        .padding(.trailing, 6)
        .padding(.vertical, 3)
        .background(fill, in: RoundedRectangle(cornerRadius: 6))
        .overlay {
            if style == .outlined {
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(border, lineWidth: 1)
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: 6))
        .onHover { isHovered = $0 }
    }

    @ViewBuilder
    private var tagChips: some View {
        switch tags {
        case .sources(let sources):
            ForEach(Array(sources.enumerated()), id: \.offset) { _, source in
                SignalSourceChip(source: source)
                    .opacity(style == .outlined ? 0.5 : 1)
            }
        case .discIdFile(let file):
            if let file {
                DiscIdFileChip(source: file)
            }
        }
    }

    private var valueStyle: AnyShapeStyle {
        style == .outlined
            ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.secondary)
    }

    private var fill: Color {
        switch style {
        case .filled: Color.primary.opacity(isHovered ? 0.06 : 0.035)
        case .outlined: Color.primary.opacity(isHovered ? 0.04 : 0)
        }
    }

    private var border: Color {
        Color.primary.opacity(isHovered ? 0.18 : 0.09)
    }
}

extension IdentifierChip where Trailing == EmptyView {
    /// A chip with nothing after its value: a number nobody has asked about
    /// yet.
    init(
        label: String,
        tags: IdentifierTags = .sources([]),
        value: String? = nil,
        style: IdentifierChipStyle = .filled
    ) {
        self.init(label: label, tags: tags, value: value, style: style) {
            EmptyView()
        }
    }
}

/// One provider's answer about one value, inside that value's chip: the
/// provider's name and its lookup's glyph, on one ground so they read as one
/// unit.
struct ProviderCapsule: View {
    let source: BridgeCatalog
    let lookup: BridgeLookupState
    let onRetry: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            Text(bridgeCatalogName(catalog: source))
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
                .fixedSize()
            LookupCellView(lookup: lookup, onRetry: onRetry)
        }
        .padding(.horizontal, 5)
        .padding(.vertical, 1)
        .background(
            Color.primary.opacity(0.05),
            in: RoundedRectangle(cornerRadius: 4)
        )
    }
}

/// The short dash that says nothing ran here.
struct IdentifierDash: View {
    var body: some View {
        RoundedRectangle(cornerRadius: 1)
            .fill(Color.primary.opacity(0.28))
            .frame(width: 8, height: 1.5)
    }
}

/// The mark a signal carries when reading its own input failed, before any
/// provider was asked. What went wrong is the chip's hover.
struct IdentifierWarning: View {
    var body: some View {
        Image(systemName: "exclamationmark.triangle")
            .font(.system(size: 11))
            .foregroundStyle(.orange)
    }
}

/// The spinner a chip carries while what feeds it is still being read.
struct ChipSpinner: View {
    var body: some View {
        ProgressView()
            .controlSize(.small)
            .scaleEffect(0.55)
            .frame(width: 10, height: 10)
    }
}

/// A chip of nothing but a spinner, closing the band: the artwork is still
/// being read, so more chips may still join it.
struct ScanningChip: View {
    var body: some View {
        ChipSpinner()
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(
                Color.primary.opacity(0.035),
                in: RoundedRectangle(cornerRadius: 6)
            )
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
            IdentifierDash()
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
                IdentifierWarning()
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

/// The caption naming what the value after it is — the one a chip opens
/// with, and the one before each further value a chip carries.
struct IdentifierLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.tertiary)
            .fixedSize()
    }
}
