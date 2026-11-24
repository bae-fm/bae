import AppKit

/// An `NSTableView` whose header's column buttons come before its rows in
/// the accessibility order: a person reading the table with VoiceOver meets
/// the column names before the cells.
class HeaderFirstTableView: NSTableView {
    override func accessibilityChildren() -> [Any]? {
        guard let children = super.accessibilityChildren() else { return nil }
        guard let header = headerView else { return children }
        let isHeader: (Any) -> Bool = { child in
            guard let view = child as? NSView else { return false }
            return view === header || view.isDescendant(of: header)
        }
        return children.filter(isHeader) + children.filter { !isHeader($0) }
    }
}
