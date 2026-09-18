import AppKit
import BaeKit
import SwiftUI

/// All files behind one source chip, in core's sighting order.
struct EvidenceViewer: View {
    let contents: [BridgeEvidenceContent]
    let onClose: () -> Void
    @State
    private var selection = 0

    var body: some View {
        VStack(spacing: 0) {
            if contents.count > 1 {
                Picker("Release Files", selection: $selection) {
                    ForEach(contents.indices, id: \.self) { index in
                        Text(name(of: contents[index])).tag(index)
                    }
                }
                .padding()
            }
            switch contents[selection] {
            case .document(let name, let text):
                DocumentViewerView(name: name, text: text, onClose: onClose)
            case .image(let name, let bytes):
                HStack {
                    Text(name)
                    Spacer()
                    Button("Done", action: onClose)
                }
                .padding()
                ImageView(
                    content: .bytes(Data(bytes)),
                    contentMode: .fit,
                    pointSize: 1000
                )
            case .reveal(let path):
                VStack(spacing: 16) {
                    Text(path).textSelection(.enabled)
                    Button("Reveal in Finder") {
                        NSWorkspace.shared.activateFileViewerSelecting([
                            URL(fileURLWithPath: path)
                        ])
                    }
                    Button("Done", action: onClose)
                }
                .padding()
            }
        }
        .frame(width: 800, height: 600)
    }

    private func name(of content: BridgeEvidenceContent) -> String {
        switch content {
        case .document(let name, _), .image(let name, _): name
        case .reveal(let path): path
        }
    }
}
