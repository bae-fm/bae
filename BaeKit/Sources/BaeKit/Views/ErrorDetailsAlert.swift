#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// The alert a notice's Details opens: the title, the message, and the
    /// causes in an inset panel with Copy in its corner. An `NSAlert`, since
    /// SwiftUI's alert holds only text and buttons; run modally it opens the
    /// same way from a window, a sheet or a popover. AppKit adds its own OK.
    @MainActor
    public enum ErrorDetailsAlert {
        /// The causes panel's width: the alert's usual 260 less its margins.
        private static let panelWidth: CGFloat = 228

        public static func show(_ details: ErrorDetails) {
            let alert = NSAlert()
            alert.messageText = details.title
            alert.informativeText = details.message ?? ""
            let panel = NSHostingView(
                rootView: ErrorCausesPanel(
                    detail: details.copyText,
                    excerpt: details.causes
                )
                .frame(width: panelWidth)
            )
            let height = panel.fittingSize.height
            panel.frame = NSRect(x: 0, y: 0, width: panelWidth, height: height)
            alert.accessoryView = panel
            alert.runModal()
        }
    }
#endif
