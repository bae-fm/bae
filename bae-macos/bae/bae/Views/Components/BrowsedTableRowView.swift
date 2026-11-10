import AppKit
import BaeKit
import SwiftUI

/// A row of a table the person browses, such as the import list or the
/// Storage Manager. It marks its selection the way every list the person
/// browses does: the accent's tint behind the row, its text and glyphs
/// keeping their own colours. AppKit's own selection fills the row with the
/// system accent, which is not the app's, and turns the row's text white.
final class BrowsedTableRowView: NSTableRowView {
    private let tint: NSColor

    /// A row whose selection is `accent`'s tint.
    init(accent: AccentChoice) {
        tint = NSColor(name: nil) { appearance in
            let dark =
                appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            return NSColor(accent.color(in: dark ? .dark : .light))
                .withAlphaComponent(ThemeOpacity.tint)
        }
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// AppKit's own selection, its shape kept and its colour replaced by
    /// the tint: the shape follows the table's style, inset and rounded in
    /// an inset table, and joins neighbouring selected rows.
    override func drawSelection(in dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else {
            return
        }
        context.beginTransparencyLayer(auxiliaryInfo: nil)
        super.drawSelection(in: dirtyRect)
        tint.setFill()
        bounds.fill(using: .sourceIn)
        context.endTransparencyLayer()
    }

    /// The tint lies behind the row's own colours, so its cells draw as on
    /// any other row.
    override var interiorBackgroundStyle: NSView.BackgroundStyle { .normal }
}
