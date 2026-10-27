import AppKit
import BaeKit
import SwiftUI

/// The import list's geometry in its table.
@MainActor
enum ImportCandidateTableLayout {
    /// Every row's height, headers and rows still loading included: a row's
    /// cover and the padding around it.
    static let rowHeight =
        TriageRowView.coverPointSize + 2 * ThemeSpace.compact
    /// The gap above a folder group, drawn inside its header's row.
    static let headerGap = ThemeSpace.related
}

/// The import list's table. It keeps no selection of its own: what a person
/// does to its rows goes to core. It names the event it is handling, for the
/// selection it proposes to be read against, and it takes Select All to
/// core, which selects every row the list shows rather than the rows the
/// table has loaded.
final class ImportCandidateTableView: NSTableView {
    /// The mouse or key press the table is handling, while it handles it.
    private(set) var handlingEvent: NSEvent?
    /// Select every row the list shows, loaded or not.
    var onSelectAll: (() -> Void)?

    override func mouseDown(with event: NSEvent) {
        handling(event) { super.mouseDown(with: event) }
    }

    override func keyDown(with event: NSEvent) {
        handling(event) { super.keyDown(with: event) }
    }

    override func selectAll(_ sender: Any?) {
        onSelectAll?()
    }

    /// The row `event` pressed, for a click on a row.
    func pressedRow(_ event: NSEvent) -> Int? {
        guard event.type == .leftMouseDown else { return nil }
        let row = row(at: convert(event.locationInWindow, from: nil))
        return row >= 0 ? row : nil
    }

    private func handling(_ event: NSEvent, _ handle: () -> Void) {
        let outer = handlingEvent
        handlingEvent = event
        defer { handlingEvent = outer }
        handle()
    }
}

/// A folder group's header row.
final class ImportListGroupRowView: NSTableRowView {
    /// The header draws its own surface, inside its content and out of the
    /// gap above it.
    override func drawBackground(in dirtyRect: NSRect) {}
}

/// A table cell hosting one row of the import list. A reused cell takes the
/// row it is given now, and draws it with increased prominence while the
/// table draws its row selected.
final class ImportListEntryCell: NSTableCellView {
    private var hosting: NSHostingView<ImportListEntryView>?
    private var entry: ImportListEntry?
    /// Whether the row floats over the rows below: the group header pinned
    /// over the top of the table while its group scrolls under it.
    var floating = false {
        didSet {
            guard floating != oldValue else { return }
            render()
        }
    }

    init(identifier: NSUserInterfaceItemIdentifier) {
        super.init(frame: .zero)
        self.identifier = identifier
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var backgroundStyle: NSView.BackgroundStyle {
        didSet {
            guard backgroundStyle != oldValue else { return }
            render()
        }
    }

    func show(_ entry: ImportListEntry) {
        self.entry = entry
        render()
    }

    private func render() {
        guard let entry else { return }
        let root = ImportListEntryView(
            entry: entry,
            prominence: backgroundStyle == .emphasized ? .increased : .standard,
            floating: floating
        )
        if let hosting {
            hosting.rootView = root
            return
        }
        let view = NSHostingView(rootView: root)
        // The row's height is the table's; the row fills it.
        view.sizingOptions = []
        view.frame = bounds
        view.autoresizingMask = [.width, .height]
        addSubview(view)
        hosting = view
    }
}
