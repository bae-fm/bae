import SwiftUI

/// One menu button per browser mode, the selected one checkmarked, for the
/// header heading's dropdown.
struct LibraryModeButtons: View {
    let selected: LibraryBrowserMode
    let select: (LibraryBrowserMode) -> Void

    var body: some View {
        ForEach(LibraryBrowserMode.allCases, id: \.self) { mode in
            Button {
                select(mode)
            } label: {
                if selected == mode {
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
        Menu {
            LibraryModeButtons(selected: .composers, select: { _ in })
        } label: {
            Text(verbatim: "Browse Mode")
        }
        .menuStyle(.button)
        .padding()
        .frame(width: 220)
    }
#endif
