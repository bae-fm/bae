import BaeKit
import SwiftUI

// The pieces the identifier band is built from.

/// Filled for a value the run asks about, outlined for one it does not, and
/// outlined and struck through for one the person struck out.
enum IdentifierChipStyle {
    case filled
    case outlined
    case struck
}

/// The widest a chip's value is drawn before it truncates in the middle.
private let identifierValueWidth: CGFloat = 92

/// The widest a chip holding several values grows before its values wrap onto
/// another line.
private let identifierListWidth: CGFloat = 280

/// One identifier in the band: its kind, its value, and the providers' answers.
struct IdentifierChip<Trailing: View>: View {
    let label: String
    var value: String?
    /// Every value the chip holds, where it holds several: all of them,
    /// separated by " · " and wrapping inside the chip, never the first alone.
    /// Takes the place of `value`.
    var values: [String] = []
    var style: IdentifierChipStyle = .filled
    /// Where each of `values` is a button, what it does: the value written
    /// in the list, separated and wrapping as the values of a chip are.
    var onValue: ((String) -> Void)?
    @ViewBuilder
    let trailing: Trailing

    @State
    private var isHovered = false

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            IdentifierLabel(text: label)
            if !values.isEmpty, let onValue {
                FlowLayout(spacing: ThemeSpace.line) {
                    ForEach(Array(values.enumerated()), id: \.offset) {
                        index,
                        value in
                        HStack(spacing: ThemeSpace.line) {
                            Button {
                                onValue(value)
                            } label: {
                                Text(value)
                                    .themeText(.mono)
                                    .foregroundStyle(valueStyle)
                            }
                            .buttonStyle(.accentPlain)
                            if index < values.count - 1 {
                                Text(verbatim: FactList.glyph)
                                    .themeText(.mono)
                                    .foregroundStyle(valueStyle)
                            }
                        }
                    }
                }
                .frame(maxWidth: identifierListWidth, alignment: .leading)
            }
            else if !values.isEmpty {
                Text(IdentifierChipValues.text(values))
                    .themeText(.mono)
                    .foregroundStyle(valueStyle)
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: identifierListWidth, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityLabel(IdentifierChipValues.spoken(values))
            }
            else if let value {
                Text(value)
                    .themeText(.mono)
                    .strikethrough(style == .struck)
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
            if style != .filled {
                RoundedRectangle(cornerRadius: ThemeRadius.chip)
                    .strokeBorder(border, lineWidth: 1)
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
        .onHover { isHovered = $0 }
    }

    private var valueStyle: AnyShapeStyle {
        style == .filled
            ? AnyShapeStyle(.secondary) : AnyShapeStyle(.tertiary)
    }

    private var fill: Color {
        switch style {
        case .filled: isHovered ? Theme.pressed : Theme.hover
        case .outlined, .struck: isHovered ? Theme.hover : Color.clear
        }
    }

    private var border: Color {
        isHovered ? Theme.hairlineStrong : Theme.hairline
    }
}

/// How a chip that holds several values writes them.
enum IdentifierChipValues {
    /// Every value, separated by " · ".
    static func text(_ values: [String]) -> String {
        values.joined(separator: " · ")
    }

    /// Every value as accessibility reads them: a list in the viewer's
    /// language.
    static func spoken(_ values: [String]) -> String {
        values.formatted(.list(type: .and, width: .standard))
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
    let source: BridgeLookupCatalog
    let lookup: BridgeLookupState
    let onRetry: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(bridgeLookupCatalogName(catalog: source))
                .themeText(.chip)
                .foregroundStyle(StatusTone.neutral.color)
                .fixedSize()
            LookupCellView(lookup: lookup, onRetry: onRetry)
        }
        .padding(.leading, ThemeSpace.compact)
        .padding(.trailing, ThemeSpace.inline)
        .padding(.vertical, ThemeSpace.hairline)
        .background(StatusTone.neutral.fill, in: Capsule())
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

/// The mark a signal carries when reading its input failed.
struct IdentifierWarning: View {
    var body: some View {
        Image(systemName: "exclamationmark.circle")
            .themeIcon(.small)
            .foregroundStyle(Theme.warning)
            .accessibilityLabel("Failed")
    }
}

/// The words a chip carries while what feeds it is still being read or
/// asked: what it waits on. Each initializer takes the state a chip shows and
/// fails for a state that is not a wait.
struct ChipWait: View {
    let words: String

    /// A Disc ID still being read off the folder's rip logs and CUE sheets.
    init?(waitingOn step: BridgeDiscIdStep) {
        guard case .reading = step else { return nil }
        words = Self.reading
    }

    /// ISRCs still being read off the files' tags.
    init?(waitingOn step: BridgeIsrcStep) {
        guard case .reading = step else { return nil }
        words = Self.reading
    }

    /// A title search waiting on the identifiers' lookups.
    init?(waitingOn step: BridgeSearchStep) {
        guard case .waiting = step else { return nil }
        words = String(
            localized: "Waiting…",
            comment:
                "Find online ledger: in the title search chip, while the run asks about the folder's identifiers before it searches by these words."
        )
    }

    /// A catalog still being asked about a value.
    init?(waitingOn lookup: BridgeLookupState) {
        guard case .lookingUp = lookup else { return nil }
        words = String(
            localized: "Searching…",
            comment:
                "Find online ledger: beside a catalog's name in an identifier's chip, while bae asks that catalog about the identifier."
        )
    }

    /// The artwork still being read, which can add barcodes and catalog
    /// numbers to the band.
    init?(readingArtworkOf run: BridgeIdentifyRun) {
        guard run.readingArtwork else { return nil }
        words = String(
            localized: "Reading artwork…",
            comment:
                "Find online ledger: the chip closing the identifier band while bae still reads text from the folder's artwork, which can add barcodes and catalog numbers."
        )
    }

    private static var reading: String {
        String(
            localized: "Reading…",
            comment:
                "Find online ledger: in the Disc ID or ISRC chip while bae reads the folder's rip logs, CUE sheets or tags for it."
        )
    }

    var body: some View {
        Text(verbatim: words)
            .themeText(.chip)
            .foregroundStyle(.tertiary)
            .fixedSize()
    }
}

/// A chip closing the band while the artwork is still being read.
struct ScanningChip: View {
    let wait: ChipWait

    var body: some View {
        wait
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
        case .notAsked:
            IdentifierDash()
        case .lookingUp:
            ChipWait(waitingOn: lookup)
        case .found(let count, let groups):
            LookupCountView(count: Int(count), groups: groups)
        case .noMatch:
            StatusChip(verbatim: 0.formatted())
        case .failed(let failure):
            HStack(spacing: ThemeSpace.compact) {
                IdentifierWarning()
                    .help(failure.badgeLine)
                Button(action: onRetry) {
                    Image(systemName: "arrow.clockwise")
                        .themeIcon(.small)
                        .foregroundStyle(Color.accentColor)
                }
                .buttonStyle(.accentPlain)
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
        StatusChip(
            verbatim: count.formatted(),
            tone: count > 0 ? .success : .neutral
        )
        .hoverPopover(arrowEdge: .bottom) {
            LookupReleasesPopover(
                groups: groups.map(ReleaseGroup.init(bridge:))
            )
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
                            Text(verbatim: "\(FactList.glyph) \(artist)")
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
            if !pressing.years.isEmpty {
                Text(PressingText.alternatives(pressing.years))
                    .themeText(.strong)
                    .monospacedDigit()
                    .lineLimit(1)
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
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                }
                ForEach(
                    Array(label.catalogNumbers.enumerated()),
                    id: \.offset
                ) { _, catalogNumber in
                    StatusChip(verbatim: catalogNumber.stated.text)
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
