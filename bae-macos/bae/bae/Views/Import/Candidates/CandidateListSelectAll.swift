import AppKit
import ObjectiveC
import SwiftUI

/// Makes Select All in the candidate list call `onSelectAll`, which asks core
/// for every row the list shows.
///
/// Edit ▸ Select All and Command-A both send `selectAll:` to the first
/// responder, which here is the table SwiftUI builds for the list. That table
/// answers it itself, selecting the rows whose pages have loaded, before
/// anything SwiftUI offers (`onCommand`) can: it is the first responder, and
/// SwiftUI's own application class ignores a principal class that could route
/// the action elsewhere. So this view finds the table drawn over it and gives
/// that one table a `selectAll:` of its own, leaving every other table and text
/// field alone. The menu item keeps the table's validation.
struct CandidateListSelectAll: NSViewRepresentable {
    let onSelectAll: @MainActor () -> Void

    func makeNSView(context _: Context) -> LocatorView { LocatorView() }

    func updateNSView(_ view: LocatorView, context _: Context) {
        view.onSelectAll = onSelectAll
        view.routeTable()
    }

    /// Sits behind the list, at its frame, to find the table drawn there.
    final class LocatorView: NSView {
        var onSelectAll: @MainActor () -> Void = {}

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            routeTable()
        }

        override func layout() {
            super.layout()
            routeTable()
        }

        /// Route the table whose scroll view has this view's frame. Run again
        /// on every layout, since SwiftUI may rebuild the table.
        func routeTable() {
            guard let window, let content = window.contentView else { return }
            let frame = convert(bounds, to: nil)
            guard frame.width > 0, frame.height > 0,
                let table = Self.tables(in: content)
                    .first(where: { table in
                        guard let scroll = table.enclosingScrollView else {
                            return false
                        }
                        let tableFrame = scroll.convert(scroll.bounds, to: nil)
                        return abs(tableFrame.minX - frame.minX) < 1
                            && abs(tableFrame.minY - frame.minY) < 1
                            && abs(tableFrame.width - frame.width) < 1
                            && abs(tableFrame.height - frame.height) < 1
                    })
            else { return }
            TableSelectAllRouting.route(table) { [weak self] in
                self?.onSelectAll()
            }
        }

        private static func tables(in view: NSView) -> [NSTableView] {
            view.subviews.flatMap { subview -> [NSTableView] in
                if let table = subview as? NSTableView { return [table] }
                return tables(in: subview)
            }
        }
    }
}

/// Gives one table instance a `selectAll:` that calls a handler instead of
/// selecting the table's rows, by moving that instance to a subclass of its
/// own class that overrides the method. Other instances of the class are
/// untouched.
@MainActor
enum TableSelectAllRouting {
    private final class Handler {
        let run: @MainActor () -> Void
        init(_ run: @escaping @MainActor () -> Void) { self.run = run }
    }

    private static let handlerKey = UnsafeMutableRawPointer.allocate(
        byteCount: 1,
        alignment: 1
    )
    private static let subclassPrefix = "BaeRoutedSelectAll_"

    static func route(
        _ table: NSTableView,
        to handler: @escaping @MainActor () -> Void
    ) {
        objc_setAssociatedObject(
            table,
            handlerKey,
            Handler(handler),
            .OBJC_ASSOCIATION_RETAIN_NONATOMIC
        )
        guard let current = object_getClass(table) else { return }
        guard !NSStringFromClass(current).hasPrefix(subclassPrefix) else {
            return
        }
        object_setClass(table, subclass(of: current))
    }

    private static func subclass(of base: AnyClass) -> AnyClass {
        let name = subclassPrefix + NSStringFromClass(base)
        if let existing = NSClassFromString(name) { return existing }
        guard let subclass = objc_allocateClassPair(base, name, 0) else {
            fatalError("could not make \(name)")
        }
        let selectAll: @convention(block) (NSTableView, Any?) -> Void = {
            table,
            _ in
            MainActor.assumeIsolated {
                let handler =
                    objc_getAssociatedObject(table, handlerKey) as? Handler
                handler?.run()
            }
        }
        class_addMethod(
            subclass,
            #selector(NSResponder.selectAll(_:)),
            imp_implementationWithBlock(selectAll),
            "v@:@"
        )
        objc_registerClassPair(subclass)
        return subclass
    }
}
