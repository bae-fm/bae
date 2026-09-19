import BaeKit
import SwiftUI

/// The document panel shared by file rows and source chips.
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
        .frame(width: 750, height: 600)
        .background(Theme.surface)
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
    }
#endif
