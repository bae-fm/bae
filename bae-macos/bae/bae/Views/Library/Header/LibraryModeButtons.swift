import SwiftUI

/// One menu item per browser mode, for the header heading's dropdown. An
/// inline picker, so the menu draws its own checkmark column for the selected
/// mode.
struct LibraryModeButtons: View {
    let selected: LibraryBrowserMode
    let select: (LibraryBrowserMode) -> Void

    var body: some View {
        Picker(
            selection: Binding(get: { selected }, set: select)
        ) {
            ForEach(LibraryBrowserMode.allCases, id: \.self) { mode in
                Text(mode.displayName).tag(mode)
            }
        } label: {
            EmptyView()
        }
        .pickerStyle(.inline)
        .labelsHidden()
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
