import BaeKit
import SwiftUI

/// A settings switch whose explanation is the row's second line.
struct SettingToggleRow: View {
    let title: LocalizedStringKey
    let note: LocalizedStringKey
    @Binding
    var isOn: Bool

    var body: some View {
        Toggle(isOn: $isOn) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(title)
                Text(note)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

#if DEBUG
    #Preview("Setting Toggle Row") {
        Form {
            Section {
                SettingToggleRow(
                    title: "Identify automatically",
                    note: "Looks up each new album as soon as it’s added.",
                    isOn: .constant(true)
                )
            }
        }
        .formStyle(.grouped)
        .frame(width: 500, height: 120)
    }
#endif
