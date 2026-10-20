import BaeKit
import SwiftUI

/// A library browser mode with nothing to show, and why. Core's album count
/// decides which: a library with no albums has nothing imported, so it offers
/// Import Folder — the same folder picker as the menu and the import tab, so
/// where the person lands afterwards is decided the same way — and Add
/// Discogs Key while Discogs can't be asked. A library with albums has simply
/// credited nobody this mode lists, and importing more is not the answer, so
/// that is all it says.
struct LibraryEmptyState: View {
    /// What the mode lists, for a library with nothing imported.
    let title: LocalizedStringKey
    /// What the mode is missing, for a library that has albums. `nil` for the
    /// albums mode itself, which is empty only when the library is.
    var nothingCredited: LocalizedStringKey?

    @Environment(UiStore.self)
    private var uiStore
    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        switch (libraryStore.albumTotal, nothingCredited) {
        case (0, _), (_, nil):
            ContentUnavailableView {
                Text(title)
            } description: {
                Text("Import some music to get started")
            } actions: {
                HStack(spacing: ThemeSpace.related) {
                    Button("Import Folder...") {
                        uiStore.setImportFolderPickerPresented(true)
                    }
                    AddDiscogsKeyButton()
                }
            }
        case (.some, .some(let nothingCredited)):
            ContentUnavailableView {
                Text(nothingCredited)
            }
        case (nil, .some):
            // Core has not counted the albums yet, so which it is is not
            // known; the title alone is true either way.
            ContentUnavailableView {
                Text(title)
            }
        }
    }
}

#if DEBUG
    /// The empty state over a library whose album count is `albumTotal`,
    /// with or without a usable Discogs key.
    @MainActor
    private func emptyStatePreview(
        albumTotal: Int,
        discogsUsable: Bool
    ) -> some View {
        let store = LibraryStore()
        store.setAlbumTotal(albumTotal)
        return LibraryEmptyState(
            title: "No composers",
            nothingCredited: "No composer credits in your library"
        )
        .environment(UiStore())
        .environment(store)
        .environment(
            PreviewData.makeConfigStore(
                libraryFullWidth: false,
                discogsUsable: discogsUsable
            )
        )
        .environment(SettingsNavigation())
        .mainWindowPreviewEnvironment()
        .frame(width: 600, height: 400)
    }

    #Preview("Library empty state") {
        emptyStatePreview(albumTotal: 0, discogsUsable: true)
    }

    #Preview("Library empty state \u{2014} no Discogs key") {
        emptyStatePreview(albumTotal: 0, discogsUsable: false)
    }

    #Preview("Library empty state \u{2014} albums, no credits") {
        emptyStatePreview(albumTotal: 12, discogsUsable: false)
    }
#endif
