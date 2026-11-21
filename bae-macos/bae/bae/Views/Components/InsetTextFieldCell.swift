import AppKit

/// A text field cell whose text sits inside the field's bounds by `insets`,
/// so the field itself spans its whole chrome: its accessibility frame, its
/// click area and its I-beam cover the padding too.
protocol InsetCell: NSTextFieldCell {
    var insets: NSSize { get set }
}

extension InsetCell {
    fileprivate func inset(_ rect: NSRect) -> NSRect {
        rect.insetBy(dx: insets.width, dy: insets.height)
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
