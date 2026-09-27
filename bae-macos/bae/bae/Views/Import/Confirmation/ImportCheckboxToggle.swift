import BaeKit
import SwiftUI

/// A checkbox-style toggle used in the confirmation header (Cloud / Pinned).
struct ImportCheckboxToggle: View {
    private let label: LocalizedStringKey
    @Binding
    var isOn: Bool

    init(_ title: LocalizedStringKey, isOn: Binding<Bool>) {
        label = title
        _isOn = isOn
    }

    var body: some View {
        Toggle(isOn: $isOn) { Text(label) }
            .toggleStyle(.checkbox)
            .themeText(.body)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Checkbox toggles") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ImportCheckboxToggle("Cloud", isOn: .constant(true))
            ImportCheckboxToggle("Pinned", isOn: .constant(false))
        }
        .padding()
        .windowBackground()
    }
#endif
