import AppKit
import BaeKit

/// A text field cell whose text sits inside the field's bounds by `insets`,
/// so the field itself spans its whole chrome: its accessibility frame, its
/// click area and its I-beam cover the padding too.
protocol InsetCell: NSTextFieldCell {
    var insets: NSSize { get set }
}

extension InsetCell {
    /// How far AppKit's own cell insets its text from the frame it draws in,
    /// on each side; measured at 1.75–1.9 points for every font the fields
    /// use.
    static var appKitTextInset: CGFloat { 2 }

    /// The frame to draw in so the text sits `insets` inside the field: the
    /// cell adds its own inset to what this leaves.
    fileprivate func inset(_ rect: NSRect) -> NSRect {
        rect.insetBy(
            dx: insets.width - Self.appKitTextInset,
            dy: insets.height
        )
    }
}

final class InsetTextFieldCell: NSTextFieldCell, InsetCell {
    var insets = NSSize.zero

    override func drawInterior(
        withFrame cellFrame: NSRect,
        in controlView: NSView
    ) {
        super.drawInterior(withFrame: inset(cellFrame), in: controlView)
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
                withFrame: inset(rect),
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
                withFrame: inset(rect),
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
        super.drawInterior(withFrame: inset(cellFrame), in: controlView)
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
                withFrame: inset(rect),
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
                withFrame: inset(rect),
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
    /// ring of its own, one clipped line, its text inset by the cell, which
    /// the chrome around it pads.
    func useAsBoxedEditor(cell: NSTextFieldCell) {
        cell.isEditable = true
        cell.isSelectable = true
        self.cell = cell
        isBordered = false
        isBezeled = false
        drawsBackground = false
        focusRingType = .none
        lineBreakMode = .byClipping
        cell.isScrollable = true
        cell.wraps = false
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
