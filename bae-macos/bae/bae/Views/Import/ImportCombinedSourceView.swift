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
            MacMenuButton(bordered: true) {
                for part in parts {
                    MacMenuItem(verbatim: part.name) {
                        SystemActions.revealInFinder(folder: part.folderPath)
                    }
                }
            } label: {
                Text("Source folders")
            }
            .fixedSize()
            Button("Keep as separate releases", action: onSeparate)
                .disabled(!canSeparate)
        }
        .padding(ThemeSpace.edge)
        .background(Theme.surfaceElevated)
    }
}
