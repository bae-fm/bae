import AppKit
import BaeKit
import SwiftUI

// The pieces the identifier band is built from.

/// What a chip says about its identifier. Off is an outline, for an
/// identifier the run leaves out. On is a fill, for one the run asks about.
/// Found is the accent's tint, for one that found releases. A chip that holds
/// no value, only a dash, has no fill and no ring.
enum MacIdentifierChipStyle {
    case off
    case on
    case found
    case dash
}

/// How much of the background shows through a value's inset.
private let identifierInsetOpacity = 0.7
/// How much of the accent shows in a found chip's fill.
private let identifierFoundOpacity = 0.26
/// How much of the primary text colour a found chip's name keeps.
private let identifierFoundNameOpacity = 0.75

extension MacIdentifierChipStyle {
    var nameColor: Color {
        switch self {
        case .off, .on, .dash: Theme.textTertiary
        case .found: Theme.textPrimary.opacity(identifierFoundNameOpacity)
        }
    }

    var valueColor: Color {
        switch self {
        case .off, .dash: Theme.textTertiary
        case .on: Color.halfway(Theme.textPrimary, Theme.textSecondary)
        case .found: Theme.textPrimary
        }
    }
}

extension Color {
    /// The colour halfway between two, taken in the appearance it is drawn
    /// in.
    fileprivate static func halfway(_ first: Color, _ second: Color) -> Color {
        Color(
            nsColor: NSColor(name: nil) { appearance in
                var halfway: NSColor?
                appearance.performAsCurrentDrawingAppearance {
                    halfway = NSColor(first).usingColorSpace(.sRGB)
                        .flatMap {
                            from in
                            NSColor(second).usingColorSpace(.sRGB)
                                .flatMap {
                                    from.blended(withFraction: 0.5, of: $0)
                                }
                        }
                }
                guard let halfway else {
                    preconditionFailure(
                        "the theme's text colours blend in sRGB"
                    )
                }
                return halfway
            }
        )
    }
}

/// One identifier in the band: its kind and its value.
struct MacIdentifierChip<Trailing: View>: View {
    let label: String
    var value: String?
    /// What the chip draws where it holds several values: the first, then +N.
    /// Takes the place of `value`.
    var values: MacIdentifierChipValues?
    var style: MacIdentifierChipStyle = .on
    /// Where pressing the chip does something, what it does. The chip's label
    /// and value stay separate elements for accessibility, as the board draws
    /// them.
    var onPress: (() -> Void)?
    /// Whether the trailing part holds controls of its own, a failed chip's
    /// Retry. Then the chip is a box of two controls, the toggle that holds
    /// the label and value, and the trailing part beside it, since one button
    /// cannot hold another.
    var separatesTrailing = false
    @ViewBuilder
    let trailing: Trailing

    var body: some View {
        if let onPress, separatesTrailing {
            MacIdentifierChipFrame(style: style) {
                // The label and value stay elements of their own, which a
                // button around them would hide, so the toggle is the
                // parts' own action.
                HStack(spacing: ThemeSpace.compact) { parts }
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onPress)
                    .accessibilityElement(children: .contain)
                    .accessibilityAction(.default, onPress)
                trailing
                    .fixedSize(horizontal: true, vertical: false)
            }
        }
        else if let onPress {
            Button(action: onPress) { content }
                .buttonStyle(.accentPlain)
        }
        else {
            content
        }
    }

    private var content: some View {
        MacIdentifierChipFrame(style: style, containsParts: style != .dash) {
            parts
            trailing
                .fixedSize(horizontal: true, vertical: false)
        }
    }

    /// The label and what the chip holds.
    @ViewBuilder
    private var parts: some View {
        IdentifierLabel(text: label, style: style)
        if let list = values {
            MacIdentifierMoreValues(list: list, style: style)
        }
        else if let value {
            IdentifierValueText(value: value, style: style)
        }
    }
}

/// The chip's box: its look's fill or outline, and the room round its parts.
struct MacIdentifierChipFrame<Content: View>: View {
    let style: MacIdentifierChipStyle
    /// Whether the chip's parts stay separate elements for accessibility. A
    /// chip of one part, as the dash chip's name, is that part's element.
    var containsParts = true
    @ViewBuilder
    let content: Content

    @State
    private var isHovered = false

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            content
        }
        .padding(.horizontal, ThemeSpace.compact + ThemeSpace.hairline)
        .padding(.vertical, ThemeSpace.line + ThemeSpace.hairline)
        .background(fill, in: RoundedRectangle(cornerRadius: ThemeRadius.chip))
        .overlay {
            if style == .off {
                RoundedRectangle(cornerRadius: ThemeRadius.chip)
                    .strokeBorder(border, lineWidth: 1)
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
        .onHover { isHovered = $0 }
        .accessibilityChildren(containing: containsParts)
    }

    private var fill: Color {
        switch style {
        case .on: isHovered ? Theme.pressed : Theme.hover
        case .found: Theme.accent.opacity(identifierFoundOpacity)
        case .dash: Color.clear
        case .off: isHovered ? Theme.hover : Color.clear
        }
    }

    private var border: Color {
        isHovered ? Theme.hairlineStrong : Theme.hairline
    }
}

extension View {
    /// Keeps the view's parts as separate elements for accessibility, or
    /// leaves them as they are.
    @ViewBuilder
    fileprivate func accessibilityChildren(containing: Bool) -> some View {
        if containing {
            accessibilityElement(children: .contain)
        }
        else {
            self
        }
    }
}

extension MacIdentifierChip where Trailing == IdentifierDash {
    /// A chip that says nothing ran here: its label, any value it holds and
    /// a dash.
    init(
        label: String,
        value: String? = nil,
        values: MacIdentifierChipValues? = nil
    ) {
        self.init(
            label: label,
            value: value,
            values: values,
            style: value == nil && values == nil ? .dash : .on
        ) {
            IdentifierDash()
        }
    }
}

/// What a chip that holds several values draws, as core hands it over: the
/// shown value, the count of the values behind the +N button, and every
/// value for the button's popover.
struct MacIdentifierChipValues: Equatable {
    let shown: String
    let moreCount: UInt32
    let all: [String]

    /// The button's words, with the count in the viewer's digits; `nil` when
    /// the chip shows every value.
    var moreTitle: String? {
        moreCount == 0 ? nil : "+" + moreCount.formatted()
    }

    /// Every value as accessibility reads them: a list in the viewer's
    /// language.
    var spoken: String {
        all.formatted(.list(type: .and, width: .standard))
    }
}

/// A chip's value on its inset: the background at 70%, in the detail type.
struct IdentifierValueText: View {
    let value: String
    let style: MacIdentifierChipStyle

    var body: some View {
        Text(value)
            .themeText(.detail)
            .foregroundStyle(style.valueColor)
            .cutLine(.prose)
            .identifierInset()
    }
}

extension View {
    /// Sets the view on a value's inset. The inset is a hairline taller than
    /// the detail line on each side and overlaps the chip's padding there, so
    /// it never makes the chip taller. `overlapsNext` pulls the next part of
    /// the chip up to it.
    func identifierInset(overlapsNext: Bool = true) -> some View {
        padding(.horizontal, ThemeSpace.compact - ThemeSpace.hairline)
            .padding(.vertical, ThemeSpace.hairline)
            .background(
                Theme.background.opacity(identifierInsetOpacity),
                in: RoundedRectangle(
                    cornerRadius: ThemeRadius.chip - ThemeSpace.hairline
                )
            )
            // A control on the inset is boxed with its padding, as layout
            // boxes it, not by the words inside.
            .contentShape(.accessibility, Rectangle())
            .padding(.vertical, -ThemeSpace.line)
            .padding(
                .trailing,
                overlapsNext ? ThemeSpace.line - ThemeSpace.compact : 0
            )
    }
}

/// The first value of a chip and, behind +N, a popover with them all. No
/// value in it is a button.
private struct MacIdentifierMoreValues: View {
    let list: MacIdentifierChipValues
    let style: MacIdentifierChipStyle

    @State
    private var showsAll = false

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            IdentifierValueText(value: list.shown, style: style)
            if let more = list.moreTitle {
                MacButton(kind: .plain) {
                    showsAll = true
                } label: {
                    Text(verbatim: more)
                        .themeText(.chip)
                        .foregroundStyle(style.nameColor)
                }
                // The button reads every value, as the first alone would
                // leave the rest unheard.
                .accessibilityHint(list.spoken)
                .macPopover(isPresented: $showsAll, arrowEdge: .bottom) {
                    MacIdentifierValuesPopover(lines: list.all)
                }
            }
        }
    }
}

/// Every value of a chip, one a line, in the detail type: a 150 point column
/// with a group space round it, set in the popover's 14 points at the sides
/// and below and 4 above, as the board draws it.
struct MacIdentifierValuesPopover: View {
    let lines: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(verbatim: line)
                    .themeText(.detail)
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

/// What trails a chip's value while one of its lookups is out or has failed,
/// as core says: nothing once every lookup answered, as the chip's look shows
/// what they found.
struct ChipTrailing: View {
    let state: BridgeChipTrailing
    let onRetry: () -> Void

    var body: some View {
        switch state {
        case .nothing:
            EmptyView()
        case .searching:
            ChipWait(searching: ())
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
                .accessibilityLabel("Retry")
                .help("Retry")
            }
        }
    }
}

extension BridgeChipTrailing {
    /// Whether the trailing part is a failure with its own Retry button.
    var isFailed: Bool {
        if case .failed = self { return true }
        return false
    }
}

extension MacIdentifierChipStyle {
    /// The look core gives a chip.
    init(_ look: BridgeChipLook) {
        switch look {
        case .off: self = .off
        case .on: self = .on
        case .found: self = .found
        }
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
    init(searching: Void) {
        words = String(
            localized: "Searching…",
            comment:
                "Find online ledger: at the end of an identifier's chip, while bae asks a catalog about the identifier."
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

/// The caption naming the value after it.
struct IdentifierLabel: View {
    let text: String
    var style: MacIdentifierChipStyle = .on

    var body: some View {
        Text(text)
            .themeText(.chip)
            .foregroundStyle(style.nameColor)
            .fixedSize()
    }
}
