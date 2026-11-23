import SwiftUI

/// The dimming behind a Mac sheet, alert or side pause: `Theme.scrim` across
/// the whole window, title bar included. Every presentation draws this one
/// scrim: a view inside the window (`MacModal`, `MacSidePause`) draws it as
/// this view, and a window that sits over another one (`MacAlert`'s panel,
/// `MacSheet`'s system sheet) draws it through `ScrimWindow`.
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
        private var scrim: ScrimWindow?
        private var observers: [NSObjectProtocol] = []

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard window != nil, observers.isEmpty else { return }
            // The sheet is attached to its window after its content is in
            // the sheet's window: a window starting or ending a sheet says
            // so, and each time the sheet may be attached or gone.
            for name in [
                NSWindow.willBeginSheetNotification,
                NSWindow.didEndSheetNotification,
                NSWindow.didBecomeKeyNotification,
            ] {
                observers.append(
                    NotificationCenter.default.addObserver(
                        forName: name,
                        object: nil,
                        queue: .main
                    ) { [weak self] _ in
                        MainActor.assumeIsolated { self?.sync() }
                        // The sheet is attached once the notice is handled.
                        DispatchQueue.main.async { [weak self] in
                            MainActor.assumeIsolated { self?.sync() }
                        }
                    }
                )
            }
            sync()
        }

        private func clear() {
            scrim?.hide()
            scrim = nil
        }

        private func sync() {
            guard let sheet = window, let parent = sheet.sheetParent else {
                clear()
                return
            }
            guard scrim == nil else { return }
            let scrim = ScrimWindow(over: parent)
            scrim.show()
            // The sheet stays in front of the scrim that dims its window.
            scrim.order(.below, relativeTo: sheet.windowNumber)
            self.scrim = scrim
        }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            if newWindow == nil {
                clear()
                observers.forEach(NotificationCenter.default.removeObserver)
                observers = []
            }
            super.viewWillMove(toWindow: newWindow)
        }
    }
#endif
