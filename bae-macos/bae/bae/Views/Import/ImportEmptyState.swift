import BaeKit
import SwiftUI

/// The Import tab with no folder watched: the sheep, Import Folder, which opens
/// the folder picker, and Add Discogs Key while Discogs can't be asked.
struct ImportEmptyState: View {
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            ImportSheep(size: 140)
                .padding(.bottom, ThemeSpace.section - ThemeSpace.group)
            Text("Add a folder to import music from")
                .themeText(.body)
                .foregroundStyle(.secondary)
            HStack(spacing: ThemeSpace.related) {
                Button("Import folder…") {
                    uiStore.setImportFolderPickerPresented(true)
                }
                AddDiscogsKeyButton()
            }
        }
        .padding(ThemeSpace.page)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
