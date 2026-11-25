import AppKit
import BaeKit

/// A text field cell whose text sits inside the field's bounds by `insets`,
/// so the field itself spans its whole chrome: its accessibility frame, its
/// click area and its I-beam cover the padding too.
protocol InsetCell: NSTextFieldCell {
    var insets: NSSize { get set }
}

extension InsetCell {
    /// The frame to draw in so the text sits `insets` inside the field. The
    /// cell adds no inset of its own to it.
    ///
    /// SwiftUI's `Text` seats the font's line at the bottom of the
    /// whole-point height it lays out, and the cell draws the line at the top
    /// of that height, so the frame moves down on screen by the difference,
    /// rounded to a whole point: a shift of 0.09 points moved the title's
    /// ink by half a point, which the board does not have.
    fileprivate func inset(_ rect: NSRect, in view: NSView) -> NSRect {
        var inset = rect.insetBy(dx: insets.width, dy: insets.height)
        if let font {
            let line = font.ascender - font.descender + font.leading
            let down = (line.rounded(.up) - line).rounded()
            inset.origin.y += view.isFlipped ? down : -down
        }
        return inset
    }
}

final class InsetTextFieldCell: NSTextFieldCell, InsetCell {
    var insets = NSSize.zero

    override func drawInterior(
        withFrame cellFrame: NSRect,
        in controlView: NSView
    ) {
        super
            .drawInterior(
                withFrame: inset(cellFrame, in: controlView),
                in: controlView
            )
    }

    override func edit(
        withFrame rect: NSRect,
        in controlView: NSView,
        editor textObj: NSText,
        delegate: Any?,
        event: NSEvent?
    ) {
        super
            .edit(
                withFrame: inset(rect, in: controlView),
                in: controlView,
                editor: textObj,
                delegate: delegate,
                event: event
            )
    }

    override func select(
        withFrame rect: NSRect,
        in controlView: NSView,
        editor textObj: NSText,
        delegate: Any?,
        start selStart: Int,
        length selLength: Int
    ) {
        super
            .select(
                withFrame: inset(rect, in: controlView),
                in: controlView,
                editor: textObj,
                delegate: delegate,
                start: selStart,
                length: selLength
            )
    }
}

/// `InsetTextFieldCell` for a secure field.
final class InsetSecureTextFieldCell: NSSecureTextFieldCell, InsetCell {
    var insets = NSSize.zero

    override func drawInterior(
        withFrame cellFrame: NSRect,
        in controlView: NSView
    ) {
        super
            .drawInterior(
                withFrame: inset(cellFrame, in: controlView),
                in: controlView
            )
    }

    override func edit(
        withFrame rect: NSRect,
        in controlView: NSView,
        editor textObj: NSText,
        delegate: Any?,
        event: NSEvent?
    ) {
        super
            .edit(
                withFrame: inset(rect, in: controlView),
                in: controlView,
                editor: textObj,
                delegate: delegate,
                event: event
            )
    }

    override func select(
        withFrame rect: NSRect,
        in controlView: NSView,
        editor textObj: NSText,
        delegate: Any?,
        start selStart: Int,
        length selLength: Int
    ) {
        super
            .select(
                withFrame: inset(rect, in: controlView),
                in: controlView,
                editor: textObj,
                delegate: delegate,
                start: selStart,
                length: selLength
            )
    }
}

extension NSTextField {
    /// Makes the field the editor of a boxed field: no bezel, background or
    /// ring of its own, one line, its text inset by the cell, which
    /// the chrome around it pads.
    func useAsBoxedEditor(cell: NSTextFieldCell) {
        cell.isEditable = true
        cell.isSelectable = true
        self.cell = cell
        isBordered = false
        isBezeled = false
        drawsBackground = false
        focusRingType = .none
        cell.isScrollable = true
        cell.wraps = false
        // A value too long for the field ends in "…" while nobody edits it;
        // typing scrolls it. Set after `isScrollable`, which sets its own.
        lineBreakMode = .byTruncatingTail
    }

    /// The placeholder in the tertiary colour, in `font`.
    func setBoxedPlaceholder(_ placeholder: String, font: NSFont) {
        placeholderAttributedString = NSAttributedString(
            string: placeholder,
            attributes: [
                .font: font,
                .foregroundColor: NSColor(Theme.textTertiary),
            ]
        )
    }
}
