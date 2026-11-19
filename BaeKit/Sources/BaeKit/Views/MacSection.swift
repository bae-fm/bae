import SwiftUI

/// A heading over a box of rows, with a footer under the box: one section of
/// a settings form.
public struct MacSection<Header: View, Content: View, Footer: View>: View {
    private let header: Header
    private let content: Content
    private let footer: Footer

    public init(
        @ViewBuilder content: () -> Content,
        @ViewBuilder header: () -> Header,
        @ViewBuilder footer: () -> Footer
    ) {
        self.content = content()
        self.header = header()
        self.footer = footer()
    }

    public var body: some View {
        Section {
            content
        } header: {
            header
        } footer: {
            footer
        }
    }
}

extension MacSection where Header == EmptyView, Footer == EmptyView {
    public init(@ViewBuilder content: () -> Content) {
        self.init(
            content: content,
            header: { EmptyView() },
            footer: { EmptyView() }
        )
    }
}

extension MacSection where Header == EmptyView {
    public init(
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.init(content: content, header: { EmptyView() }, footer: footer)
    }
}

extension MacSection where Footer == EmptyView {
    public init(
        @ViewBuilder content: () -> Content,
        @ViewBuilder header: () -> Header
    ) {
        self.init(content: content, header: header) { EmptyView() }
    }
}

extension MacSection where Header == Text, Footer == EmptyView {
    public init(
        _ title: LocalizedStringKey,
        @ViewBuilder content: () -> Content
    ) {
        self.init(content: content) { Text(title) }
    }
}
