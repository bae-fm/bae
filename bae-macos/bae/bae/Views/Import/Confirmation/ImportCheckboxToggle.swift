import SwiftUI

/// A checkbox-style toggle used in the confirmation header (Managed / Keep
/// local copy).
struct ImportCheckboxToggle: View {
    private let label: Text
    @Binding
    var isOn: Bool

    init(_ title: LocalizedStringKey, isOn: Binding<Bool>) {
        label = Text(title)
        _isOn = isOn
    }

    /// A label bae-core supplied, already in the user's language — a catalog
    /// key resolved rather than a key of this app's own.
    init(core title: String, isOn: Binding<Bool>) {
        label = Text(verbatim: title)
        _isOn = isOn
    }

    var body: some View {
        Toggle(isOn: $isOn) { label }
            .toggleStyle(.checkbox)
            .font(.callout)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Checkbox toggles") {
        VStack(alignment: .leading, spacing: 8) {
            ImportCheckboxToggle("Managed", isOn: .constant(true))
            ImportCheckboxToggle("Keep local copy", isOn: .constant(false))
        }
        .padding()
        .windowBackground()
    }
#endif
