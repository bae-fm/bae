#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// The alert a notice's Details opens: the title, the message, and the
    /// causes in an inset panel with Copy in its corner. An `NSAlert`, since
    /// SwiftUI's alert holds only text and buttons; run modally it opens the
    /// same way from a window, a sheet or a popover. AppKit adds its own OK.
    @MainActor
    public enum ErrorDetailsAlert {
        public static func show(_ details: ErrorDetails) {
            makeAlert(details).runModal()
        }

        /// The alert as `show` runs it, before it runs.
        public static func makeAlert(_ details: ErrorDetails) -> NSAlert {
            let alert = NSAlert()
            alert.messageText = details.title
            alert.informativeText = details.message ?? ""
            alert.accessoryView = AlertAccessory.view(
                HoverErrorDetails(
                    detail: details.copyText,
                    excerpt: details.causes
                )
            )
            return alert
        }
    }

    /// How a notice shows its details. The default opens the alert; a panel
    /// that must close before the alert runs modally sets its own.
    public struct ShowErrorDetailsAction {
        private let handler: @MainActor (ErrorDetails) -> Void

        public init(_ handler: @escaping @MainActor (ErrorDetails) -> Void) {
            self.handler = handler
        }

        @MainActor
        public func callAsFunction(_ details: ErrorDetails) {
            handler(details)
        }
    }

    extension EnvironmentValues {
        @Entry
        public var showErrorDetails = ShowErrorDetailsAction {
            ErrorDetailsAlert.show($0)
        }
    }

    /// The inset panel of an error's causes, one a line and ending in the fault,
    /// with Copy details in its corner, shown under the pointer or keyboard
    /// focus.
    public struct MacErrorDetails: View {
        let detail: String
        let excerpt: String
        /// Whether the pointer is over the panel, which shows Copy details.
        let hover: Bool
        @FocusState
        private var copyFocused: Bool

        public init(detail: String, excerpt: String, hover: Bool = false) {
            self.detail = detail
            self.excerpt = excerpt
            self.hover = hover
        }

        public var body: some View {
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
                    Theme.insetPanel,
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
                    .focused($copyFocused)
                    .opacity(hover || copyFocused ? 1 : 0)
                    .padding(ThemeSpace.related)
                }
        }
    }

    /// A `MacErrorDetails` that follows the pointer.
    private struct HoverErrorDetails: View {
        let detail: String
        let excerpt: String
        @State
        private var hovering = false

        var body: some View {
            MacErrorDetails(detail: detail, excerpt: excerpt, hover: hovering)
                .onHover { hovering = $0 }
        }
    }
#endif
