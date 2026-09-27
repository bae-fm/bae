import BaeKit
import SwiftUI

// The pieces the identifier band is built from.

/// Filled for a value the run asks about, outlined for one it does not.
enum IdentifierChipStyle {
    case filled
    case outlined
}

/// The widest a chip's value is drawn before it truncates in the middle.
private let identifierValueWidth: CGFloat = 92

/// One identifier in the band: its kind, its value, and the providers' answers.
struct IdentifierChip<Trailing: View>: View {
    let label: String
    var value: String?
    var style: IdentifierChipStyle = .filled
    @ViewBuilder
    let trailing: Trailing

    @State
    private var isHovered = false

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            IdentifierLabel(text: label)
            if let value {
                Text(value)
                    .themeText(.mono)
                    .foregroundStyle(valueStyle)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: identifierValueWidth, alignment: .leading)
            }
            trailing
        }
        .padding(.horizontal, ThemeSpace.compact)
        .padding(.vertical, ThemeSpace.line)
        .background(fill, in: RoundedRectangle(cornerRadius: ThemeRadius.chip))
        .overlay {
            if style == .outlined {
                RoundedRectangle(cornerRadius: ThemeRadius.chip)
                    .strokeBorder(border, lineWidth: 1)
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
        .onHover { isHovered = $0 }
    }

    private var valueStyle: AnyShapeStyle {
        style == .outlined
            ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.secondary)
    }

    private var fill: Color {
        switch style {
        case .filled: isHovered ? Theme.pressed : Theme.hover
        case .outlined: isHovered ? Theme.hover : Color.clear
        }
    }

    private var border: Color {
        isHovered ? Theme.hairlineStrong : Theme.hairline
    }
}

extension IdentifierChip where Trailing == EmptyView {
    /// A chip with nothing after its value.
    init(
        label: String,
        value: String? = nil,
        style: IdentifierChipStyle = .filled
    ) {
        self.init(label: label, value: value, style: style) {
            EmptyView()
        }
    }
}

/// One provider's name and its lookup's glyph, inside a value's chip.
struct ProviderCapsule: View {
    let source: BridgeCatalog
    let lookup: BridgeLookupState
    let onRetry: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(bridgeCatalogName(catalog: source))
                .themeText(.chip)
                .foregroundStyle(.secondary)
                .fixedSize()
            LookupCellView(lookup: lookup, onRetry: onRetry)
        }
        .padding(.horizontal, ThemeSpace.compact)
        .padding(.vertical, ThemeSpace.hairline)
        .background(
            Theme.hover,
            in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
        )
    }
}

/// The dash that says nothing ran here.
struct IdentifierDash: View {
    var body: some View {
        RoundedRectangle(cornerRadius: ThemeRadius.bar)
            .fill(Theme.hairlineStrong)
            .frame(width: 8, height: 1.5)
    }
}

/// The mark a step carries when it is switched off in Settings.
struct IdentifierOff: View {
    var body: some View {
        Text("Off")
            .themeText(.chip)
            .foregroundStyle(.tertiary)
            .fixedSize()
    }
}

/// The mark a signal carries when reading its input failed.
struct IdentifierWarning: View {
    var body: some View {
        Image(systemName: "exclamationmark.triangle")
            .themeIcon(.small)
            .foregroundStyle(Theme.warning)
    }
}

/// The spinner a chip carries while what feeds it is still being read.
struct ChipSpinner: View {
    var body: some View {
        ProgressView()
            .controlSize(.small)
            .scaleEffect(0.55)
            .frame(width: ThemeIcon.small.size, height: ThemeIcon.small.size)
    }
}

/// A spinner closing the band while the artwork is still being read.
struct ScanningChip: View {
    var body: some View {
        ChipSpinner()
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background(
                Theme.hover,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
    }
}

/// One provider's lookup of one value as a glyph.
struct LookupCellView: View {
    let lookup: BridgeLookupState
    let onRetry: () -> Void

    var body: some View {
        switch lookup {
        case .queued:
            Circle()
                .fill(Theme.hairlineStrong)
                .frame(width: 5, height: 5)
        case .notAsked(reason: .switchedOff):
            IdentifierOff()
                .help("Switched off in Import settings")
        case .notAsked(reason: .leftOut), .notAsked(reason: .noCatalog):
            IdentifierDash()
        case .lookingUp:
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.6)
                .frame(
                    width: ThemeIcon.small.size,
                    height: ThemeIcon.small.size
                )
        case .found(let count, let groups):
            LookupCountView(count: Int(count), groups: groups)
        case .noMatch:
            CountCapsule(count: 0)
        case .failed(let failure):
            HStack(spacing: ThemeSpace.compact) {
                IdentifierWarning()
                    .help(failure.badgeLine)
                Button(action: onRetry) {
                    Image(systemName: "arrow.clockwise")
                        .themeIcon(.small)
                        .foregroundStyle(Color.accentColor)
                }
                .buttonStyle(.plain)
                .help("Retry")
            }
        }
    }
}

/// A match count that shows the releases it stands for on hover.
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

/// The releases one lookup found, headed by album when there are several.
struct LookupReleasesPopover: View {
    let groups: [ReleaseGroup]

    private static let thumbnailSize: CGFloat = 16

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
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
                            .padding(.horizontal, ThemeSpace.line)
                            .padding(.vertical, ThemeSpace.inline)
                    }
                    HStack(spacing: ThemeSpace.compact) {
                        ImageView(
                            content: group.coverImageContent,
                            pointSize: Self.thumbnailSize
                        )
                        .frame(
                            width: Self.thumbnailSize,
                            height: Self.thumbnailSize
                        )
                        .clipShape(
                            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        )
                        Text(group.title)
                            .themeText(.rowTitle)
                            .lineLimit(1)
                        if let artist = group.artist {
                            Text(verbatim: "\u{00b7} \(artist)")
                                .themeText(.detail)
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                        }
                    }
                    .padding(.horizontal, ThemeSpace.compact)
                    .padding(.top, ThemeSpace.inline)
                    .padding(.bottom, ThemeSpace.line)
                    ForEach(group.pressings) { pressing in
                        // Under the album's title, past its thumbnail.
                        LookupReleaseLine(pressing: pressing)
                            .padding(
                                .leading,
                                Self.thumbnailSize + ThemeSpace.compact
                            )
                    }
                }
            }
        }
        .padding(ThemeSpace.compact)
        .frame(width: 264)
    }
}

/// One pressing a lookup found: the facts that tell pressings of one album
/// apart.
struct LookupReleaseLine: View {
    let pressing: Pressing

    private var pressed: String {
        pressing.summaryText
    }

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            if let year = pressing.lead.year {
                Text(String(year))
                    .themeText(.strong)
                    .monospacedDigit()
            }
            ForEach(Array(pressing.labels.enumerated()), id: \.offset) {
                _,
                label in
                if let name = label.name {
                    Text(name)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                ForEach(label.catalogNumbers, id: \.self) { catalogNumber in
                    Text(catalogNumber)
                        .themeText(.chip)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, ThemeSpace.inline)
                        .background(
                            Theme.hover,
                            in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
                        )
                        .lineLimit(1)
                }
            }
            if !pressed.isEmpty {
                Text(pressed)
                    .themeText(.fine)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, ThemeSpace.compact)
        .padding(.vertical, ThemeSpace.line)
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

/// The caption naming the value after it.
struct IdentifierLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .themeText(.chip)
            .foregroundStyle(.tertiary)
            .fixedSize()
    }
}
