import BaeKit
import SwiftUI

/// One choice of a group: a radio, filled when `checked`, then the choice's
/// name with a line under it that says what it is. The group's button around
/// it makes the choice.
struct MacRadio: View {
    let title: Text
    var note: Text?
    let checked: Bool

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related + ThemeSpace.line) {
            MacRadioMark(checked: checked)
                .padding(.top, ThemeSpace.hairline)

            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                title
                    .themeText(.body)
                if let note {
                    note
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                }
            }

            Spacer()
        }
        .contentShape(Rectangle())
    }
}
