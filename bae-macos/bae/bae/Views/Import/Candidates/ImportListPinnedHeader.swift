import AppKit
import BaeKit

/// The group header pinned over the top of the import list: the header of
/// the group the row on top is listed under. Each member names its header by
/// the header's key, so the header is known from the row on top alone,
/// whichever windows are loaded; a window that starts inside a group delivers
/// that group's header beside its rows.
struct ImportListPinnedHeader: Equatable {
    /// The header's key.
    let key: String
    /// The header, when a loaded row or a window's leading header is it.
    let header: BridgeImportListItem?

    /// The header pinned while the row at `top` is on top: a header on top
    /// pins itself, and a release in no group, or a row that has not landed,
    /// pins none. `header` finds the header of a key among the rows and
    /// leading headers the list holds.
    static func over(
        top: Int,
        item: (Int) -> BridgeImportListItem?,
        header: (String) -> BridgeImportListItem?
    ) -> ImportListPinnedHeader? {
        guard let key = headerKey(of: top, item: item) else { return nil }
        return ImportListPinnedHeader(key: key, header: header(key))
    }

    /// Whether the group ends with the row at `top`: the row below it has
    /// landed under another header, or under none. The pinned header then
    /// leaves with the row on top as it scrolls away.
    func ends(below top: Int, item: (Int) -> BridgeImportListItem?) -> Bool {
        guard item(top + 1) != nil else { return false }
        return Self.headerKey(of: top + 1, item: item) != key
    }

    /// The key of the header the row at `row` is listed under: the row's own
    /// for a header, its group's header for a member.
    private static func headerKey(
        of row: Int,
        item: (Int) -> BridgeImportListItem?
    ) -> String? {
        switch item(row) {
        case .groupHeader(let stableKey, _, _, _, _):
            stableKey
        case .candidate(_, _, let headerKey?):
            headerKey
        case .candidate, .invalid, nil:
            nil
        }
    }
}

/// The strip over the top of the import list's rows that the pinned header
/// draws in: the header's own row. The strip cuts the header off as it leaves with the last row of
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
