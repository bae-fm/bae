import AppKit
import BaeKit

/// The group header pinned over the top of the import list: the header of
/// the group the row on top is listed under. Each member carries its group's
/// header and whether it is the last row the list shows of the group, and
/// each header whether the list shows no member under it, so the header is
/// known from the row on top alone, whichever rows the list holds.
struct ImportListPinnedHeader: Equatable {
    let header: BridgeImportGroupHeader
    /// Whether the row on top is the last row the list shows of its group:
    /// the pinned header then leaves with that row as it scrolls away.
    let ends: Bool

    /// The header pinned while `item` is the row on top: a header on top
    /// pins itself, and a release in no group, or a row that is not loaded,
    /// pins none.
    static func over(top item: BridgeImportListItem?) -> ImportListPinnedHeader?
    {
        switch item {
        case .groupHeader(let header):
            ImportListPinnedHeader(header: header, ends: header.showsNoMember)
        case .candidate(_, _, let group?):
            ImportListPinnedHeader(header: group.header, ends: group.last)
        case .candidate, .invalid, nil:
            nil
        }
    }
}

/// The strip over the top of the import list's rows that the pinned header
/// draws in: the header's own row. The strip cuts the header off as it
/// leaves with the last row of its group.
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
        let height = ImportCandidateTableLayout.headerRowHeight
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
            cell.show(entry, displayScale: window?.backingScaleFactor)
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
