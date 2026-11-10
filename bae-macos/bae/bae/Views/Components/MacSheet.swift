import BaeKit
import SwiftUI

/// A sheet's face: its title in bold at the top, the line under it in
/// secondary text, then its content and its footer. The line is what the
/// sheet asks for or reports, as an alert's message is.
struct MacSheet<Content: View, Footer: View>: View {
    let title: LocalizedStringKey
    let message: LocalizedStringKey?
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
        self.title = title
        self.message = message
        self.width = width
        self.content = content()
        self.footer = footer()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text(title)
                .themeText(.strong)
            if let message {
                SheetMessage(message)
            }
            content
            footer
        }
        .padding(ThemeSpace.edge)
        .frame(width: width, alignment: .leading)
    }
}

#if DEBUG
    #Preview("Sheet") {
        MacSheet(
            "Sheet title",
            message: "The line under the title.",
            width: 400
        ) {
            TextField(text: .constant(""), prompt: Text(verbatim: "Field")) {
                EmptyView()
            }
            .boxedTextField()
        } footer: {
            MacSheetFooter {
                Button("Cancel") {}
                Button("Continue") {}
                    .buttonStyle(PrimaryButtonStyle())
            }
        }
    }
#endif
