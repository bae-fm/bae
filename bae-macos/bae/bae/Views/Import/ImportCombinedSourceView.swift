import BaeKit
import SwiftUI

/// Above the pane of a release read from several folders: the folders it is
/// made of, and the action that reads them as releases of their own.
struct ImportCombinedSourceView: View {
    let parts: [BridgeReleasePart]
    let canSeparate: Bool
    let onSeparate: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.edge) {
            Label("Combined folders", systemImage: "square.stack.3d.up")
                .themeText(.heading)
            Spacer()
            Menu("Source folders") {
                ForEach(parts, id: \.folderPath) { part in
                    Button(part.name) {
                        SystemActions.revealInFinder(path: part.folderPath)
                    }
                }
            }
            .fixedSize()
            Button("Keep as Separate Releases", action: onSeparate)
                .disabled(!canSeparate)
        }
        .padding(ThemeSpace.edge)
        .background(Theme.surfaceElevated)
    }
}
