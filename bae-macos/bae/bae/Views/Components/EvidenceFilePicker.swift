import BaeKit
import SwiftUI

/// Chooses a file when several sightings contributed to one source chip.
struct EvidenceFilePicker: View {
    let contents: [BridgeEvidenceContent]
    let onSelect: (BridgeEvidenceContent) -> Void
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Release Files").font(.headline)
                Spacer()
                Button("Done", action: onClose)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(contents.indices, id: \.self) { index in
                        Button(action: { onSelect(contents[index]) }) {
                            switch contents[index] {
                            case .document(let name, _), .image(let name, _):
                                Text(verbatim: name)
                            case .reveal(let path):
                                Text(verbatim: path)
                            }
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(16)
        .frame(width: 500, height: 300)
        .background(Theme.surface)
    }
}
