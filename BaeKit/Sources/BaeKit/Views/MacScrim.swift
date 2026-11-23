import SwiftUI

/// The dimming behind a Mac sheet, alert or side pause: `Theme.scrim` across
/// the whole window, title bar included. Every presentation draws this one
/// scrim: a view inside the window (`MacModal`, `MacSidePause`) draws it as
/// this view, and a window that sits over another one (`MacAlert`'s panel,
/// `MacSheet`'s system sheet) draws it through `WindowScrim`.
public struct MacScrim: View {
    public init() {}

    public var body: some View {
        Theme.scrim.ignoresSafeArea()
    }
}

#if canImport(AppKit)
    import AppKit

    /// Dims the window a system sheet hangs from while the sheet shows: put
    /// it in the sheet's content.
    public struct SheetScrim: NSViewRepresentable {
        public init() {}

        public func makeNSView(context: Context) -> NSView {
            SheetScrimAnchor()
        }

        public func updateNSView(_ view: NSView, context: Context) {}
    }

    private final class SheetScrimAnchor: NSView {
        private var scrim: NSView?
        private var keyObserver: NSObjectProtocol?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard window != nil else { return }
            // The sheet is attached to its window after its content is in
            // the sheet's window, and a window becomes key as a sheet starts
            // and as it ends, so each key change checks whether the sheet is
            // attached.
            keyObserver = NotificationCenter.default.addObserver(
                forName: NSWindow.didBecomeKeyNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.sync() }
            }
            sync()
        }

        private func sync() {
            guard let parent = window?.sheetParent else {
                scrim?.removeFromSuperview()
                scrim = nil
                return
            }
            if scrim == nil { scrim = WindowScrim.show(over: parent) }
        }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            if newWindow == nil {
                scrim?.removeFromSuperview()
                scrim = nil
                keyObserver.map(NotificationCenter.default.removeObserver)
                keyObserver = nil
            }
            super.viewWillMove(toWindow: newWindow)
        }
    }
#endif
