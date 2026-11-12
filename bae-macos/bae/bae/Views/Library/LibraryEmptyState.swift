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
    @Environment(LibraryBrowseSession.self)
    private var session

    var body: some View {
        switch (session.albumTotal, nothingCredited) {
        case (0, _), (_, nil):
            VStack(spacing: ThemeSpace.group) {
                Text(title)
                    .themeText(.title)
                Text("Import some music to get started")
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                HStack(spacing: ThemeSpace.related) {
                    Button("Import folder…") {
                        uiStore.setImportFolderPickerPresented(true)
                    }
                    AddDiscogsKeyButton()
                }
                .padding(.top, ThemeSpace.inline)
            }
            .multilineTextAlignment(.center)
            .padding(ThemeSpace.page)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case (.some, .some(let nothingCredited)):
            PlaceholderTitle(nothingCredited)
        case (nil, .some):
            // Core has not counted the albums yet, so which it is is not
            // known; the title alone is true either way.
            PlaceholderTitle(title)
        }
    }
}

/// A pane with nothing to show, saying so in a centred title alone.
struct PlaceholderTitle: View {
    let title: LocalizedStringKey

    init(_ title: LocalizedStringKey) {
        self.title = title
    }

    var body: some View {
        Text(title)
            .themeText(.contentTitle)
            .multilineTextAlignment(.center)
            .padding(ThemeSpace.page)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
