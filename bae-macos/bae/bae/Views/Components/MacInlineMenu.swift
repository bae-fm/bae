import BaeKit
import SwiftUI

/// A choice that applies to a whole list, shown as its current value in the
/// accent with a ⌄, which opens a menu of the choices: History's whose plays,
/// at the end of its first day's heading. `action` sets it in body text, as
/// an empty list's action under its line.
struct MacInlineMenu: View {
    let value: String
    var action = false
    let items: () -> [MacMenuEntry]

    init(
        _ value: String,
        action: Bool = false,
        @MacMenuBuilder items: @escaping () -> [MacMenuEntry]
    ) {
        self.value = value
        self.action = action
        self.items = items
    }

    var body: some View {
        MacMenuButton(content: items) {
            HStack(spacing: ThemeSpace.inline) {
                Text(verbatim: value)
                    .themeText(action ? .body : .cell)
                Image(systemName: "chevron.down")
                    .font(.system(size: action ? 9 : 7, weight: .semibold))
            }
            .foregroundStyle(Theme.accent)
            .contentShape(Rectangle())
        }
        .fixedSize()
    }
}
