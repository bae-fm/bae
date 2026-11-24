import AppKit
import BaeKit
import SwiftUI

/// A column header of the storage table as the board draws it: the title in
/// the detail text and secondary colour, the sort's chevron after it, a
/// hairline between columns, and no fill of AppKit's own.
final class StorageHeaderCell: NSTableHeaderCell {
    /// The header's text is this far in from its column's edge.
    private static let padding = ThemeSpace.compact
    /// The chevron is this far after the title.
    private static let chevronGap = ThemeSpace.inline

    private var titleFont: NSFont { ThemeText.detail.nsFont }

    override init(textCell: String) {
        super.init(textCell: textCell)
        font = ThemeText.detail.nsFont
        textColor = NSColor(Theme.textSecondary)
        lineBreakMode = .byTruncatingTail
        usesSingleLineMode = true
    }

    @available(*, unavailable)
    required init(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(withFrame cellFrame: NSRect, in controlView: NSView) {
        // The first column has no column before it to be separated from.
        if cellFrame.minX > 1 {
            NSColor(Theme.hairline).setFill()
            NSRect(
                x: cellFrame.minX,
                y: cellFrame.minY,
                width: ThemeSpace.hairline,
                height: cellFrame.height
            )
            .fill()
        }
        drawInterior(withFrame: cellFrame, in: controlView)
    }

    override func drawInterior(
        withFrame cellFrame: NSRect,
        in controlView: NSView
    ) {
        let title = NSAttributedString(
            string: stringValue,
            attributes: [
                .font: titleFont,
                .foregroundColor: NSColor(Theme.textSecondary),
            ]
        )
        let size = title.size()
        let leading = Self.padding
        let x =
            alignment == .right
            ? cellFrame.maxX - Self.padding - size.width
            : cellFrame.minX + leading
        let width = min(size.width, cellFrame.width - 2 * Self.padding)
        title.draw(
            in: NSRect(
                x: x,
                y: cellFrame.midY - size.height / 2,
                width: width,
                height: size.height
            )
        )
    }

    override func sortIndicatorRect(forBounds rect: NSRect) -> NSRect {
        let size = NSSize(width: 9, height: 9)
        let text = NSAttributedString(
            string: stringValue,
            attributes: [.font: titleFont]
        )
        .size()
        let x =
            alignment == .right
            ? rect.maxX - Self.padding - text.width - Self.chevronGap
                - size.width
            : rect.minX + Self.padding + text.width + Self.chevronGap
        return NSRect(
            x: x,
            y: rect.midY - size.height / 2,
            width: size.width,
            height: size.height
        )
    }

    override func drawSortIndicator(
        withFrame cellFrame: NSRect,
        in controlView: NSView,
        ascending: Bool,
        priority: Int
    ) {
        guard
            let chevron = NSImage(
                systemSymbolName: ascending ? "chevron.down" : "chevron.up",
                accessibilityDescription: nil
            )?
            .withSymbolConfiguration(
                NSImage.SymbolConfiguration(pointSize: 8, weight: .semibold)
            )
        else {
            return
        }
        let tinted = chevron.tinted(NSColor(Theme.textSecondary))
        tinted.draw(in: cellFrame)
    }
}

extension NSImage {
    /// The image's shape filled in `color`.
    fileprivate func tinted(_ color: NSColor) -> NSImage {
        NSImage(size: size, flipped: false) { rect in
            self.draw(in: rect)
            color.set()
            rect.fill(using: .sourceAtop)
            return true
        }
    }
}
