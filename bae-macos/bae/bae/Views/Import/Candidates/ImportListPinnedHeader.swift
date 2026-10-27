import AppKit
import BaeKit

/// The group header pinned over the top of the import list: the header of
/// the group the row on top is listed under. Each member names where its
/// header sits in the list, so the header is known from the row on top alone,
/// whichever pages have landed, while the header's own row may still be on
/// its way.
struct ImportListPinnedHeader: Equatable {
    /// Where the header sits in the list.
    let position: Int
    /// The header, once the page holding it has landed.
    let header: BridgeImportListItem?

    /// The header pinned while the row at `top` is on top, read from the rows
    /// that have landed: a header on top pins itself, and a release in no
    /// group, or a row that has not landed, pins none.
    static func over(
        top: Int,
        item: (Int) -> BridgeImportListItem?
    ) -> ImportListPinnedHeader? {
        guard let position = headerPosition(of: top, item: item) else {
            return nil
        }
        return ImportListPinnedHeader(
            position: position,
            header: item(position)
        )
    }

    /// Whether the group ends with the row at `top`: the row below it has
    /// landed under another header, or under none. The pinned header then
    /// leaves with the row on top as it scrolls away.
    func ends(below top: Int, item: (Int) -> BridgeImportListItem?) -> Bool {
        guard item(top + 1) != nil else { return false }
        return Self.headerPosition(of: top + 1, item: item) != position
    }

    /// Where the header the row at `row` is listed under sits: the row itself
    /// for a header, its group's header for a member.
    private static func headerPosition(
        of row: Int,
        item: (Int) -> BridgeImportListItem?
    ) -> Int? {
        switch item(row) {
        case .groupHeader:
            row
        case .candidate(_, _, let headerPosition?):
            Int(headerPosition)
        case .candidate, .invalid, nil:
            nil
        }
    }
}

/// The strip over the top of the import list's rows that the pinned header
/// draws in: the header's own row, drawn floating, without the gap above its
/// group. The strip cuts the header off as it leaves with the last row of
/// its group.
final class ImportListPinnedHeaderStrip: NSView {
    private let cell = ImportListEntryCell(
        identifier: NSUserInterfaceItemIdentifier("pinned-header")
    )
    /// The header the cell was last given, which it draws until another
    /// replaces it or the strip forgets it.
    private var shown: BridgeImportListItem?

    init() {
        super.init(frame: .zero)
        clipsToBounds = true
        isHidden = true
        cell.floating = true
        addSubview(cell)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Draw `entry`'s header over the top of `scrollView`'s rows, `lift`
    /// points above it.
    func show(
        _ entry: ImportListEntry,
        lift: CGFloat,
        in scrollView: NSScrollView
    ) {
        let height = ImportCandidateTableLayout.rowHeight
        let rows = scrollView.contentView.frame
        frame = NSRect(
            x: rows.minX,
            y: scrollView.isFlipped ? rows.minY : rows.maxY - height,
            width: rows.width,
            height: height
        )
        cell.frame = NSRect(
            x: 0,
            y: isFlipped ? -lift : lift,
            width: rows.width,
            height: height
        )
        if entry.item != shown {
            shown = entry.item
            cell.show(entry)
        }
        isHidden = false
    }

    /// Pin nothing.
    func hide() {
        isHidden = true
    }

    /// Draw the header anew when it is next shown: the rows were drawn anew,
    /// in a new appearance or with new actions.
    func forget() {
        shown = nil
    }
}
