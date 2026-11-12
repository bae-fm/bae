import BaeKit
import SwiftUI

/// A settings switch whose explanation is the row's second line, with room
/// after the title for a help button.
struct SettingToggleRow<Help: View>: View {
    let title: LocalizedStringKey
    let note: LocalizedStringKey?
    @Binding
    var isOn: Bool
    /// Whether the switch can be moved; the help beside it stays usable.
    let canChange: Bool
    @ViewBuilder
    let help: Help

    init(
        title: LocalizedStringKey,
        note: LocalizedStringKey? = nil,
        isOn: Binding<Bool>,
        canChange: Bool = true,
        @ViewBuilder help: () -> Help = { EmptyView() }
    ) {
        self.title = title
        self.note = note
        self._isOn = isOn
        self.canChange = canChange
        self.help = help()
    }

    var body: some View {
        HStack(alignment: .center, spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                HStack(spacing: ThemeSpace.inline) {
                    Text(title)
                    help
                }
                if let note {
                    Text(note)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
            Toggle(isOn: $isOn) { Text(title) }
                .labelsHidden()
                .disabled(!canChange)
        }
    }
}

#if DEBUG
    #Preview("Setting Toggle Row") {
        Form {
            Section {
                SettingToggleRow(
                    title: "Identify automatically",
                    note: "Looks up each new album as soon as it is added.",
                    isOn: .constant(true)
                )
            }
        }
        .formStyle(.settings)
        .frame(width: 500, height: 120)
    }
#endif
