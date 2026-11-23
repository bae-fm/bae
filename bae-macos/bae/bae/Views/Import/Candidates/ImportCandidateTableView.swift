import AppKit
import BaeKit
import SwiftUI

/// The import list's geometry in its table.
@MainActor
enum ImportCandidateTableLayout {
    /// The height of a row without a full text stack, including a row
    /// whose page is still loading.
    static let rowHeight = MacImportRow.coverPointSize + verticalPadding

    private static let verticalPadding = 2 * ThemeSpace.compact
    private static let threeLineRowHeight = 50 + verticalPadding

    static func height(for row: BridgeTriageRow) -> CGFloat {
        height(
            hasArtistLine: row.metadataSummary?.artistLine != nil,
            hasStateLine: row.live.status?.line != nil
        )
    }

    static func height(for row: BridgeInvalidCandidate) -> CGFloat {
        height(hasArtistLine: row.tags?.artist != nil, hasStateLine: true)
    }

    private static func height(
        hasArtistLine: Bool,
        hasStateLine: Bool
    ) -> CGFloat {
        hasArtistLine && hasStateLine ? threeLineRowHeight : rowHeight
    }
    /// A folder group's header row: the gap above its group, the 13 pt name's
    /// 16 pt line, and the gap below it.
    static let headerPadding = EdgeInsets(
        top: ThemeSpace.related,
        leading: 0,
        bottom: ThemeSpace.inline,
        trailing: 0
    )
    static let headerRowHeight: CGFloat =
        headerPadding.top + 16 + headerPadding.bottom
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
    /// The header draws its own surface, inside its content.
    override func drawBackground(in dirtyRect: NSRect) {}
}

/// A table cell hosting one row of the import list. A reused cell takes the
/// row it is given now.
final class ImportListEntryCell: NSTableCellView {
    private var hosting: NSHostingView<ImportListEntryView>?
    private var entry: ImportListEntry?
    /// The decode of the shown row's cover, which starts when the table gives
    /// the cell its row, before the table puts the row on screen.
    private var coverDecode: Task<Void, Never>?
    init(identifier: NSUserInterfaceItemIdentifier) {
        super.init(frame: .zero)
        self.identifier = identifier
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// The item the cell draws now.
    var shownItem: BridgeImportListItem? { entry?.item }

    /// Draw `entry`, and ask the image store to decode its cover at
    /// `displayScale`, the scale of the window the cell goes on, so the
    /// cover is ready when the cell is on screen. With no scale, the row's
    /// image view loads the cover when the view draws.
    func show(_ entry: ImportListEntry, displayScale: CGFloat?) {
        self.entry = entry
        decodeCover(at: displayScale)
        render()
    }

    private func decodeCover(at displayScale: CGFloat?) {
        coverDecode?.cancel()
        coverDecode = nil
        guard let entry, let cover = entry.coverContent, let displayScale
        else { return }
        let imageStore = entry.environment.imageStore
        coverDecode = Task {
            await imageStore.warm(
                [cover],
                pointSize: MacImportRow.coverPointSize,
                displayScale: displayScale
            )
        }
    }

    private func render() {
        guard let entry else { return }
        let root = ImportListEntryView(entry: entry)
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
