import SwiftUI

/// Borderless dropdown that selects the library browser mode (Albums /
/// Composers). Styled to match `SortCriterionChip`'s label so it sits inline
/// with the sort chips in the library header. Reads and writes mode through the
/// environment `UiStore`.
struct LibraryModeMenu: View {
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Menu {
            LibraryModeButtons(uiStore: uiStore) { mode in
                uiStore.setLibraryBrowserMode(mode)
            }
        } label: {
            HStack(spacing: 2) {
                Text(uiStore.libraryBrowserMode.displayName)
                Image(systemName: "chevron.down")
            }
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }
}

/// One menu button per browser mode, each checkmarked when it is the active
/// mode. The header dropdown and the View-menu commands share this list; they
/// differ only in what selecting a mode does, so the action is injected.
struct LibraryModeButtons: View {
    let uiStore: UiStore
    let select: (LibraryBrowserMode) -> Void

    var body: some View {
        ForEach(LibraryBrowserMode.allCases, id: \.self) { mode in
            Button {
                select(mode)
            } label: {
                if uiStore.libraryBrowserMode == mode {
                    Label(mode.displayName, systemImage: "checkmark")
                }
                else {
                    Text(mode.displayName)
                }
            }
        }
    }
}

#if DEBUG
    #Preview {
        LibraryModeMenu()
            .padding()
            .environment(UiStore())
    }
#endif
