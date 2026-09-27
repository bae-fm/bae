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
        HStack(spacing: 6) {
            IdentifierLabel(text: label)
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
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.tertiary)
            .fixedSize()
    }
}

/// The mark a signal carries when reading its input failed.
struct IdentifierWarning: View {
    var body: some View {
        Image(systemName: "exclamationmark.triangle")
            .font(.system(size: 11))
            .foregroundStyle(Theme.warning)
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

/// A spinner closing the band while the artwork is still being read.
struct ScanningChip: View {
    var body: some View {
        ChipSpinner()
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
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
                        .clipShape(
                            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        )
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

/// One pressing a lookup found: the facts that tell pressings of one album
/// apart.
struct LookupReleaseLine: View {
    let pressing: Pressing

    private var pressed: String {
        pressing.summaryText
    }

    var body: some View {
        HStack(spacing: 7) {
            if let year = pressing.lead.year {
                Text(String(year))
                    .font(.system(size: 11.5, weight: .semibold))
                    .monospacedDigit()
            }
            ForEach(Array(pressing.labels.enumerated()), id: \.offset) {
                _,
                label in
                if let name = label.name {
                    Text(name)
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                ForEach(label.catalogNumbers, id: \.self) { catalogNumber in
                    Text(catalogNumber)
                        .font(.system(size: 9.5, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 4)
                        .background(
                            Theme.hover,
                            in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
                        )
                        .lineLimit(1)
                }
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

/// The caption naming the value after it.
struct IdentifierLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.tertiary)
            .fixedSize()
    }
}
