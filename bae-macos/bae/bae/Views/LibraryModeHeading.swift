import SwiftUI

/// The library screen's page heading, doubling as the browser-mode switcher.
/// The big bold label *is* the current mode ("Albums" / "Composers") and its
/// dropdown switches modes, so the switcher stays in one fixed spot as the
/// content below swaps. Reads and writes mode through the environment `UiStore`.
struct LibraryModeHeading: View {
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Menu {
            LibraryModeButtons(uiStore: uiStore) { mode in
                uiStore.setLibraryBrowserMode(mode)
            }
        } label: {
            Text(uiStore.libraryBrowserMode.displayName)
                .font(.system(size: 40, weight: .heavy))
                .tracking(-1)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }
}

#if DEBUG
    #Preview {
        LibraryModeHeading()
            .padding()
            .environment(UiStore())
    }
#endif
