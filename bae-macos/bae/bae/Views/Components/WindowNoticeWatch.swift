import AppKit

/// A registration for notices of one window, its resizes unless `notices`
/// names others: `changed` runs on the main thread after each, and the
/// registration ends when the watch is freed. A view that watches the window
/// it is in keeps the watch, so the watch ends with the view, including when
/// a closing window frees its views without moving them out of it first.
final class WindowNoticeWatch {
    private let tokens: [any NSObjectProtocol]

    init(
        of window: NSWindow,
        notices: [Notification.Name] = [NSWindow.didResizeNotification],
        changed: @escaping @MainActor (NSWindow) -> Void
    ) {
        tokens = notices.map { name in
            NotificationCenter.default.addObserver(
                forName: name,
                object: window,
                queue: .main
            ) { [weak window] _ in
                // A notice that arrives as the window goes away has nothing
                // to report.
                guard let window else {
                    return
                }
                MainActor.assumeIsolated {
                    changed(window)
                }
            }
        }
    }

    deinit {
        for token in tokens {
            NotificationCenter.default.removeObserver(token)
        }
    }
}
