import BaeKit
import SwiftUI

/// Modal text viewer for a candidate's document (a `.cue`, `.log`, `.txt`) —
/// monospaced, selectable, with a `Done` button. Presented over the import file
/// pane when a document row is tapped.
struct DocumentViewerView: View {
    let name: String
    let text: String
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(name)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Done") { onClose() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            Divider()
            ScrollView {
                Text(text)
                    .font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding()
            }
        }
    }
}

#if DEBUG
    #Preview("Document Viewer") {
        DocumentViewerView(
            name: "info.txt",
            text:
                "This is sample document content.\nLine 2 of the document.\nLine 3 with more text.",
            onClose: {},
        )
        .frame(width: 600, height: 500)
        .background(Theme.surface)
    }
#endif
