import SwiftUI

/// One menu button per browser mode, the selected one checkmarked. The header
/// heading dropdown and the View-menu commands share this list; they differ
/// in where the selected mode is read from (the View menu reads it through
/// `MenuBar`) and in what selecting a mode does, so both are passed in.
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
