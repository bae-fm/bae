import AppKit
import BaeKit

/// One value of a name pattern, drawn as the design system's accent chip:
/// the value's name in the accent, on the accent's tint, with the chip's
/// small corners and no ×. A name pattern's field draws each value as one, in
/// its text; VoiceOver reads it by the value's name.
final class MacToken: NSTextAttachmentCell {
    private let label: String
    private let accent: NSColor

    /// The space inside the chip at each side of the name.
    private static let padding: CGFloat = 8
    /// The space outside the chip at each side, between it and the text.
    private static let margin: CGFloat = 2
    /// The chip's height: one line of body text.
    private static var height: CGFloat {
        font.naturalLineHeight
    }

    private static var font: NSFont { ThemeText.body.nsFont }

    init(_ label: String, accent: NSColor) {
        self.label = label
        self.accent = accent
        super.init(textCell: "")
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError()
    }

    private var attributedLabel: NSAttributedString {
        NSAttributedString(
            string: label,
            attributes: [.font: Self.font, .foregroundColor: accent]
        )
    }

    override func cellSize() -> NSSize {
        NSSize(
            width: ceil(attributedLabel.size().width) + 2 * Self.padding
                + 2 * Self.margin,
            height: Self.height
        )
    }

    /// The chip sits on the text's line: its bottom is the font's descender
    /// below the baseline.
    override func cellBaselineOffset() -> NSPoint {
        NSPoint(x: 0, y: Self.font.descender - (Self.height - lineHeight) / 2)
    }

    private var lineHeight: CGFloat {
        Self.font.ascender - Self.font.descender
    }

    override func draw(withFrame cellFrame: NSRect, in _: NSView?) {
        let chip = cellFrame.insetBy(dx: Self.margin, dy: 0)
        let tint = accent.withAlphaComponent(ThemeOpacity.tint)
        tint.setFill()
        NSBezierPath(
            roundedRect: chip,
            xRadius: ThemeRadius.chip,
            yRadius: ThemeRadius.chip
        )
        .fill()
        let text = attributedLabel
        let size = text.size()
        text.draw(
            at: NSPoint(
                x: chip.midX - size.width / 2,
                y: chip.midY - size.height / 2
            )
        )
    }

    override func draw(
        withFrame cellFrame: NSRect,
        in controlView: NSView?,
        characterIndex _: Int,
        layoutManager _: NSLayoutManager
    ) {
        draw(withFrame: cellFrame, in: controlView)
    }

    override func accessibilityLabel() -> String? {
        label
    }
}
