import BaeKit
import SwiftUI

struct ImportCombinedSourceView: View {
    let combination: BridgeCombinationPreview
    let canSeparate: Bool
    let onSeparate: () -> Void
    @State
    private var confirming = false

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 6) {
                Label("Combined folders", systemImage: "square.stack.3d.up")
                    .font(.headline)
                Text(
                    "Separate the folders to change file roles or CUE bindings."
                )
                .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Menu("Source folders") {
                ForEach(combination.parts, id: \.candidateKey) { part in
                    Button(part.folderName) {
                        SystemActions.revealInFinder(path: part.candidateKey)
                    }
                }
            }
            .fixedSize()
            Button("Separate Folders") { confirming = true }
                .disabled(!canSeparate)
        }
        .padding(16)
        .background(Theme.surfaceElevated)
        .alert("Separate Folders?", isPresented: $confirming) {
            Button("Separate Folders", role: .destructive, action: onSeparate)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                "The combined draft will be discarded. The original folders and their metadata drafts will return; source files are unchanged."
            )
        }
    }
}
