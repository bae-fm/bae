import AppKit

/// A registration for the resizes of one window: `changed` runs on the main
/// thread after each resize, and the registration ends when the watch is
/// freed. A view that watches the window it is in keeps the watch, so the
/// watch ends with the view, including when a closing window frees its views
/// without moving them out of it first.
final class WindowResizeWatch {
    private let token: any NSObjectProtocol

    init(
        of window: NSWindow,
        changed: @escaping @MainActor (NSWindow) -> Void
    ) {
        token = NotificationCenter.default.addObserver(
            forName: NSWindow.didResizeNotification,
            object: window,
            queue: .main
        ) { [weak window] _ in
            // A resize that arrives as the window goes away has nothing to
            // report.
            guard let window else {
                return
            }
            MainActor.assumeIsolated {
                changed(window)
            }
        }
    }

    deinit {
        NotificationCenter.default.removeObserver(token)
    }
}
