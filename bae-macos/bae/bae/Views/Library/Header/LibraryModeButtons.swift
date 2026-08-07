import SwiftUI

/// One menu button per browser mode, each checkmarked when it is the active
/// mode. The header heading dropdown and the View-menu commands share this
/// list; they differ only in what selecting a mode does, so the action is
/// injected.
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
    #Preview("Library Mode Buttons") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.composers)
        return Menu {
            LibraryModeButtons(uiStore: uiStore, select: { _ in })
        } label: {
            Text(verbatim: "Browse Mode")
        }
        .menuStyle(.button)
        .padding()
        .frame(width: 220)
    }
#endif
