import BaeKit
import SwiftUI

// The pieces the identifier band is built from.

/// Filled for a value the run asks about, outlined for one it does not, and
/// outlined and struck through for one the person struck out. A chip that
/// holds no value, only a dash, has no fill and no ring.
enum MacIdentifierChipStyle {
    case filled
    case dash
    case outlined
    case struck
}

/// The widest a chip's value is drawn before it truncates in the middle.
private let identifierValueWidth: CGFloat = 92

/// One identifier in the band: its kind, its value, and the providers' answers.
struct MacIdentifierChip<Trailing: View>: View {
    let label: String
    var value: String?
    /// Every value the chip holds, where it holds several: all of them,
    /// separated by " · " and wrapping inside the chip, never the first alone.
    /// Takes the place of `value`.
    var values: [String] = []
    var style: MacIdentifierChipStyle = .filled
    /// Where pressing the chip does something, what it does. The chip's label,
    /// value and answers stay separate elements for accessibility, as the
    /// board draws them.
    var onPress: (() -> Void)?
    @ViewBuilder
    let trailing: Trailing

    @State
    private var isHovered = false

    var body: some View {
        if let onPress {
            Button(action: onPress) { content }
                .buttonStyle(.accentPlain)
        }
        else {
            content
        }
    }

    private var content: some View {
        HStack(spacing: ThemeSpace.compact) {
            IdentifierLabel(text: label)
            if let list = MacIdentifierChipValues(values) {
                MacIdentifierMoreValues(
                    list: list,
                    style: style,
                    valueStyle: valueStyle
                )
            }
            else if let value {
                IdentifierValueText(
                    value: value,
                    style: style,
                    valueStyle: valueStyle
                )
            }
            // The catalogs' capsules keep their full width.
            trailing
                .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.horizontal, ThemeSpace.compact)
        .padding(.vertical, ThemeSpace.line)
        .background(fill, in: RoundedRectangle(cornerRadius: ThemeRadius.chip))
        .overlay {
            if style == .outlined || style == .struck {
                RoundedRectangle(cornerRadius: ThemeRadius.chip)
                    .strokeBorder(border, lineWidth: 1)
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
        .onHover { isHovered = $0 }
        .accessibilityElement(children: .contain)
    }

    /// A value is in the primary text colour whatever the chip's style, as
    /// its board draws it: the style shows in the fill, the outline and the
    /// strike, not in the value's colour.
    private var valueStyle: AnyShapeStyle {
        AnyShapeStyle(Theme.textPrimary)
    }

    private var fill: Color {
        switch style {
        case .filled: isHovered ? Theme.pressed : Theme.hover
        case .dash: Color.clear
        case .outlined, .struck: isHovered ? Theme.hover : Color.clear
        }
    }

    private var border: Color {
        isHovered ? Theme.hairlineStrong : Theme.hairline
    }
}

extension MacIdentifierChip where Trailing == IdentifierDash {
    /// A chip that says nothing ran here: its label, any value it holds and
    /// a dash.
    init(label: String, value: String? = nil, values: [String] = []) {
        self.init(
            label: label,
            value: value,
            values: values,
            style: value == nil && values.isEmpty ? .dash : .filled
        ) {
            IdentifierDash()
        }
    }
}

/// How a chip that holds several values writes them: the first, then a +N
/// button with the count of the rest, which opens every one, a line each.
struct MacIdentifierChipValues {
    let first: String
    let rest: [String]

    /// `nil` for a chip that holds no value.
    init?(_ values: [String]) {
        guard let first = values.first else { return nil }
        self.first = first
        rest = Array(values.dropFirst())
    }

    /// The button's words, the count of the values the chip does not show;
    /// `nil` when it shows them all.
    var moreTitle: String? {
        rest.isEmpty ? nil : "+" + rest.count.formatted()
    }

    /// What the popover lists: every value, one a line.
    var lines: [String] { [first] + rest }

    /// Every value as accessibility reads them: a list in the viewer's
    /// language.
    var spoken: String {
        lines.formatted(.list(type: .and, width: .standard))
    }
}

/// A chip's value in the mono face, cut at the chip's value width.
private struct IdentifierValueText: View {
    let value: String
    let style: MacIdentifierChipStyle
    let valueStyle: AnyShapeStyle

    var body: some View {
        Text(value)
            .themeText(.mono)
            .strikethrough(style == .struck)
            .foregroundStyle(valueStyle)
            .cutLine(.prose)
            .frame(maxWidth: identifierValueWidth, alignment: .leading)
            // A short value takes its own width, as the board's does.
            .fixedSize(horizontal: true, vertical: false)
    }
}

/// The first value of a chip and, behind +N, a popover with them all. No
/// value in it is a button.
private struct MacIdentifierMoreValues: View {
    let list: MacIdentifierChipValues
    let style: MacIdentifierChipStyle
    let valueStyle: AnyShapeStyle

    @State
    private var showsAll = false

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            IdentifierValueText(
                value: list.first,
                style: style,
                valueStyle: valueStyle
            )
            if let more = list.moreTitle {
                MacButton(kind: .plain) {
                    showsAll = true
                } label: {
                    Text(verbatim: more)
                        .themeText(.chip)
                }
                .accent()
                // The button reads every value, as the first alone would
                // leave the rest unheard.
                .accessibilityHint(list.spoken)
                .macPopover(isPresented: $showsAll, arrowEdge: .bottom) {
                    MacIdentifierValuesPopover(lines: list.lines)
                }
            }
        }
    }
}

/// Every value of a chip, one a line, in the mono face: a 150 point column
/// with a group space round it, set in the popover's 14 points at the sides
/// and below and 4 above, as the board draws it.
struct MacIdentifierValuesPopover: View {
    let lines: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(verbatim: line)
                    .themeText(.mono)
                    .foregroundStyle(Theme.textPrimary)
            }
        }
        .padding(ThemeSpace.group)
        .frame(width: 150, alignment: .leading)
        .padding(.horizontal, ThemeSpace.group + ThemeSpace.line)
        .padding(.top, ThemeSpace.inline)
        .padding(.bottom, ThemeSpace.group + ThemeSpace.line)
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
            .foregroundStyle(Theme.textTertiary)
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
                        .foregroundStyle(Theme.accent)
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
                        MacHairline()
                            .padding(.horizontal, ThemeSpace.line)
                            .padding(.vertical, ThemeSpace.inline)
                    }
                    HStack(spacing: ThemeSpace.compact) {
                        Artwork(
                            content: group.coverImageContent,
                            pointSize: Self.thumbnailSize,
                            cornerRadius: nil
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
                                .foregroundStyle(Theme.textTertiary)
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
/// apart, as its edition line and then what it is made of.
struct LookupReleaseLine: View {
    let pressing: Pressing

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            MacSourcedValue(parts: pressing.editionParts)
                .themeText(.detail)
                .foregroundStyle(Theme.textSecondary)
                .monospacedDigit()
                .lineLimit(1)
            MacSourcedValue(parts: pressing.copyParts)
                .themeText(.fine)
                .foregroundStyle(Theme.textTertiary)
                .lineLimit(1)
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
            .foregroundStyle(Theme.textTertiary)
            .fixedSize()
    }
}
