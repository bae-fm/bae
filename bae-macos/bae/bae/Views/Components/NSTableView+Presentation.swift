import AppKit

extension NSTableView {
    /// Both axes scroll when the table outgrows its viewport. The table's
    /// containing view supplies the surface behind the scroll view.
    func themedScrollView() -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.documentView = self
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        return scrollView
    }

    /// Show the sort on its header without replacing an unchanged descriptor.
    func showSortIndicator(key: String, ascending: Bool) {
        let descriptor = NSSortDescriptor(key: key, ascending: ascending)
        if sortDescriptors.first != descriptor {
            sortDescriptors = [descriptor]
        }
    }
}
