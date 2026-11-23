import BaeKit
import SwiftUI

/// The board's sheet is a dialog named by its title. A sheet with no title
/// draws its fields, which name the task.
private struct SheetName: ViewModifier {
    let title: Text?

    func body(content: Content) -> some View {
        if let title {
            content
                .accessibilityElement(children: .contain)
                .accessibilityLabel(title)
        }
        else {
            content
        }
    }
}

/// A sheet's face: its title in bold at the top, the line under it in
/// secondary text, then its content and its footer. The line is what the
/// sheet asks for or reports, as an alert's message is.
struct MacSheet<Content: View, Footer: View>: View {
    private struct Header {
        let title: Text
        let message: Text?
    }

    private let header: Header?
    let width: CGFloat
    private let content: Content
    private let footer: Footer

    init(
        _ title: LocalizedStringKey,
        message: LocalizedStringKey? = nil,
        width: CGFloat,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.init(
            header: Header(
                title: Text(title),
                message: message.map { Text($0) }
            ),
            width: width,
            content: content,
            footer: footer
        )
    }

    /// A sheet titled with text that is not a localized key, such as a
    /// person's name.
    init(
        verbatim title: String,
        message: LocalizedStringKey? = nil,
        width: CGFloat,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.init(
            header: Header(
                title: Text(verbatim: title),
                message: message.map { Text($0) }
            ),
            width: width,
            content: content,
            footer: footer
        )
    }

    /// A sheet whose title and line the caller words, such as a title that
    /// names a receiver over a line core localized.
    init(
        title: Text,
        message: Text,
        width: CGFloat,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.init(
            header: Header(title: title, message: message),
            width: width,
            content: content,
            footer: footer
        )
    }

    /// A sheet whose fields name the task, such as the save format editor.
    init(
        width: CGFloat,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.init(
            header: nil,
            width: width,
            content: content,
            footer: footer
        )
    }

    private init(
        header: Header?,
        width: CGFloat,
        content: () -> Content,
        footer: () -> Footer
    ) {
        self.header = header
        self.width = width
        self.content = content()
        self.footer = footer()
    }

    var body: some View {
        // The boards' sheet: the title starts 18 points below the top edge,
        // the body starts 12 points below the title, the body's parts have 10
        // points between them, and the footer has its own 4 above its
        // buttons.
        VStack(
            alignment: .leading,
            spacing: ThemeSpace.related + ThemeSpace.line
        ) {
            if let header {
                header.title
                    .themeText(.strong)
                    .padding(.bottom, ThemeSpace.line)
                if let message = header.message {
                    SheetMessage(message)
                }
            }
            content
            footer
        }
        .padding([.horizontal, .bottom], ThemeSpace.edge)
        .padding(
            .top,
            header == nil ? ThemeSpace.group : ThemeSpace.edge + ThemeSpace.line
        )
        .frame(width: width, alignment: .leading)
        .modifier(SheetName(title: header?.title))
        .background(Theme.surfaceElevated)
        // The sheet is a window of its own, so the scrim over the window it
        // hangs from is drawn from here.
        .background { SheetScrim() }
        .presentationBackground(.clear)
    }
}
