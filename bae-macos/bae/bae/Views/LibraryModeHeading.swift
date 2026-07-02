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
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(uiStore.libraryBrowserMode.displayName)
                    .font(.system(size: 36, weight: .bold))
                Image(systemName: "chevron.down")
                    .font(.system(size: 20, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
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
