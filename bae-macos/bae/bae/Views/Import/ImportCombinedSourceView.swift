import BaeKit
import SwiftUI

/// Above the pane of a release read from several folders: the folders it is
/// made of, and the action that reads them as releases of their own.
struct ImportCombinedSourceView: View {
    let parts: [BridgeReleasePart]
    let canSeparate: Bool
    let onSeparate: () -> Void
    @State
    private var confirming = false

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            Label("Combined folders", systemImage: "square.stack.3d.up")
                .font(.headline)
            Spacer()
            Menu("Source folders") {
                ForEach(parts, id: \.folderPath) { part in
                    Button(part.name) {
                        SystemActions.revealInFinder(path: part.folderPath)
                    }
                }
            }
            .fixedSize()
            Button("Keep as Separate Releases") { confirming = true }
                .disabled(!canSeparate)
        }
        .padding(16)
        .background(Theme.surfaceElevated)
        .alert("Keep as Separate Releases", isPresented: $confirming) {
            Button(
                "Keep as Separate Releases",
                role: .destructive,
                action: onSeparate
            )
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                "The combined draft will be discarded. The original folders and their metadata drafts will return; source files are unchanged."
            )
        }
    }
}
