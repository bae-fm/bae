import AppKit
import BaeKit
import SwiftUI

/// An `NSTableCellView` that draws its content through a single reused
/// `NSHostingView`. Dequeued per column so SwiftUI cell views (covers, badges,
/// labels) render inside the `NSTableView`.
final class HostingTableCell: NSTableCellView {
    private var hosting: NSHostingView<AnyView>?

    func host(_ content: some View) {
        let wrapped = AnyView(content)
        if let hosting {
            hosting.rootView = wrapped
            return
        }
        let view = NSHostingView(rootView: wrapped)
        view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(
                equalTo: leadingAnchor,
                constant: ThemeSpace.inline
            ),
            view.trailingAnchor.constraint(
                equalTo: trailingAnchor,
                constant: -ThemeSpace.inline
            ),
            view.topAnchor.constraint(equalTo: topAnchor),
            view.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        hosting = view
    }
}
