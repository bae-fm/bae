#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// How a notice shows its details when something other than the notice
    /// does: a popover that must close before the alert opens sets one in the
    /// environment. Without one, the notice opens the alert itself.
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
        public var showErrorDetails: ShowErrorDetailsAction?
    }

    /// What a view with a Details button holds: calling it opens the alert,
    /// through the environment's `showErrorDetails` when one is set and else
    /// by presenting the alert from the view, which applies
    /// `errorDetailsAlert(_:)` with `presented`.
    @MainActor
    struct ErrorDetailsOpener: DynamicProperty {
        @Environment(\.showErrorDetails)
        private var hosted
        @State
        private var opened: ErrorDetails?

        var presented: Binding<ErrorDetails?> { $opened }

        func callAsFunction(_ details: ErrorDetails) {
            if let hosted {
                hosted(details)
            }
            else {
                opened = details
            }
        }
    }

    extension ErrorDetails {
        /// The alert Details opens: the title, the message, the causes in an
        /// inset panel with Copy, and OK.
        public var alert: (MacAlertWords, [MacAlertButton]) {
            (
                MacAlertWords(
                    Text(verbatim: title),
                    message: message.map { Text(verbatim: $0) },
                    details: MacAlertWords.Details(
                        copyText: copyText,
                        causes: causes
                    )
                ),
                [MacAlertButton(Text("OK"))]
            )
        }
    }

    extension View {
        /// Shows the alert a notice's Details opens, while `details` is set.
        public func errorDetailsAlert(_ details: Binding<ErrorDetails?>)
            -> some View
        {
            macAlert(item: details) { $0.alert }
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
                    in: RoundedRectangle(cornerRadius: ThemeRadius.field)
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
    struct HoverErrorDetails: View {
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
