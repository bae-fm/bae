import AppKit
import BaeKit
import SwiftUI

/// A column header of the storage table as the board draws it: the title in
/// the detail text and secondary colour, the sort's chevron after it, a
/// hairline between columns, and no fill of AppKit's own. A column at the
/// right packs the title and the chevron against its right edge.
final class StorageHeaderCell: NSTableHeaderCell {
    /// The header's text is this far in from its column's edge.
    private static let padding = ThemeSpace.compact
    /// The chevron is this far after the title.
    private static let chevronGap = ThemeSpace.inline
    private static let chevronSize = NSSize(width: 9, height: 9)

    /// The sort descriptor key of this header's column, which the table's
    /// sort names when it sorts by the column.
    var sortKey: String?

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

    /// Whether the table sorts by this column, and if so whether upward.
    private func sortedAscending(in controlView: NSView) -> Bool? {
        guard let table = (controlView as? NSTableHeaderView)?.tableView,
            let sort = table.sortDescriptors.first,
            sort.key == sortKey
        else {
            return nil
        }
        return sort.ascending
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
        let title = NSAttributedString(
            string: stringValue,
            attributes: [
                .font: titleFont,
                .foregroundColor: NSColor(Theme.textSecondary),
            ]
        )
        let size = title.size()
        let ascending = sortedAscending(in: controlView)
        let chevronRoom =
            ascending == nil
            ? 0 : Self.chevronGap + Self.chevronSize.width
        let content = size.width + chevronRoom
        let start =
            alignment == .right
            ? cellFrame.maxX - Self.padding - content
            : cellFrame.minX + Self.padding
        title.draw(
            in: NSRect(
                x: start,
                y: cellFrame.midY - size.height / 2,
                width: min(
                    size.width,
                    cellFrame.width - 2 * Self.padding - chevronRoom
                ),
                height: size.height
            )
        )
        if let ascending {
            drawChevron(
                ascending: ascending,
                in: NSRect(
                    x: start + size.width + Self.chevronGap,
                    y: cellFrame.midY - Self.chevronSize.height / 2,
                    width: Self.chevronSize.width,
                    height: Self.chevronSize.height
                )
            )
        }
    }

    /// The board draws the chevron down for an ascending sort.
    private func drawChevron(ascending: Bool, in rect: NSRect) {
        let name = ascending ? "chevron.down" : "chevron.up"
        guard
            let symbol = NSImage(
                systemSymbolName: name,
                accessibilityDescription: nil
            )?
            .withSymbolConfiguration(
                NSImage.SymbolConfiguration(pointSize: 8, weight: .semibold)
            )
        else {
            preconditionFailure("the system has the \(name) symbol")
        }
        symbol.tinted(NSColor(Theme.textSecondary)).draw(in: rect)
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
