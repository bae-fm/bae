import SwiftUI

/// A checkbox-style toggle used in the confirmation header (Managed / Keep
/// local copy).
struct ImportCheckboxToggle: View {
    let title: LocalizedStringKey
    @Binding
    var isOn: Bool

    init(_ title: LocalizedStringKey, isOn: Binding<Bool>) {
        self.title = title
        _isOn = isOn
    }

    var body: some View {
        Toggle(title, isOn: $isOn)
            .toggleStyle(.checkbox)
            .font(.callout)
    }
}
