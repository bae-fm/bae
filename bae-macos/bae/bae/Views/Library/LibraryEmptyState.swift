import SwiftUI

/// A library browser mode with nothing to show because nothing has been
/// imported, and the way to import from right here. The Import Folder button
/// opens the same folder picker as the menu and the import tab, so where the
/// person lands afterwards — the album, if the library already has it, or its
/// candidates in the import tab — is decided the same way.
struct LibraryEmptyState: View {
    let title: LocalizedStringKey

    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        ContentUnavailableView {
            Text(title)
        } description: {
            Text("Import some music to get started")
        } actions: {
            Button("Import Folder...") {
                uiStore.setImportFolderPickerPresented(true)
            }
        }
    }
}

#if DEBUG
    #Preview("Library empty state") {
        LibraryEmptyState(title: "No albums")
            .environment(UiStore())
            .frame(width: 600, height: 400)
    }
#endif
