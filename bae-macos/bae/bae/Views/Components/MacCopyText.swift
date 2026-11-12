import BaeKit
import SwiftUI

/// Text someone may want to paste elsewhere, such as the address a Subsonic
/// app connects to: selectable, with a copy button after it that shows while
/// the pointer is over the text (`hover`), or while the button has keyboard
/// focus. The button keeps its place while hidden, so the text never moves.
/// Copying puts the text on the pasteboard and says nothing; the text is
/// right there.
struct MacCopyText: View {
    let text: String
    /// Words that follow the text but aren't copied, such as which network an
    /// address is on.
    var after: String?
    var hover = false
    let copy: @MainActor (String) -> Void

    @FocusState
    private var buttonFocused: Bool

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(verbatim: text)
                .textSelection(.enabled)
            if let after {
                Text(verbatim: after)
            }
            Button {
                copy(text)
            } label: {
                Image(systemName: "doc.on.doc")
                    .themeIcon(.small)
            }
            .buttonStyle(IconHoverButtonStyle())
            .focused($buttonFocused)
            .opacity(hover || buttonFocused ? 1 : 0)
            .accessibilityLabel(Text("Copy"))
        }
        // An address is one line: it never breaks at its punctuation.
        .lineLimit(1)
        .fixedSize()
    }
}

/// A `MacCopyText` that follows the pointer: its copy button shows while the
/// pointer is over it.
struct HoverCopyText: View {
    let text: String
    var after: String?
    let copy: @MainActor (String) -> Void

    @State
    private var hovering = false

    var body: some View {
        MacCopyText(text: text, after: after, hover: hovering, copy: copy)
            .onHover { hovering = $0 }
    }
}

#if DEBUG
    #Preview("Copy text") {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            MacCopyText(text: "http://192.168.1.20:4533", copy: { _ in })
            MacCopyText(
                text: "http://100.64.0.3:4533",
                after: "(Tailscale)",
                copy: { _ in }
            )
        }
        .padding(ThemeSpace.section)
    }
#endif
