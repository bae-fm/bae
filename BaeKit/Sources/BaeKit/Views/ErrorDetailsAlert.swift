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

    /// The inset panel of an error's causes, one a line and ending in the fault,
    /// with Copy details in its corner.
    private struct ErrorCausesPanel: View {
        let detail: String
        let excerpt: String
        @Environment(\.colorScheme)
        private var colorScheme

        var body: some View {
            let text = Color.secondary
            Text(excerpt)
                .themeText(.mono)
                .foregroundStyle(text)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, ThemeSpace.related)
                .padding(.leading, ThemeSpace.related + ThemeSpace.line)
                .padding(.trailing, 30)
                .background(
                    Color.black.opacity(colorScheme == .dark ? 0.28 : 0.07),
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
                .overlay(alignment: .topTrailing) {
                    Button {
                        Pasteboard.copy(detail)
                    } label: {
                        Label("Copy details", systemImage: "doc.on.doc")
                            .labelStyle(.iconOnly)
                            .themeIcon(.small)
                            .foregroundStyle(text)
                    }
                    .buttonStyle(.borderless)
                    .help("Copy details")
                    .padding(ThemeSpace.related)
                }
        }
    }
#endif
