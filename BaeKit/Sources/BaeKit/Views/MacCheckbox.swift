import SwiftUI

/// A checkbox and its label, for an on/off choice in a row of controls. On the
/// Mac it is drawn in the accent the person chose.
public struct MacCheckbox<Label: View>: View {
    @Binding
    private var isOn: Bool
    private let label: Label
    private let role: ThemeText

    /// `detail` sets the label in the detail type, as an alert's checkbox
    /// is; otherwise it is in the body type.
    public init(
        isOn: Binding<Bool>,
        detail: Bool = false,
        @ViewBuilder label: () -> Label
    ) {
        self._isOn = isOn
        self.role = detail ? .detail : .body
        self.label = label()
    }

    public var body: some View {
        Toggle(isOn: $isOn) { label }
            .themeText(role)
            #if os(macOS)
                .toggleStyle(.accentCheckbox)
            #endif
    }
}

extension MacCheckbox where Label == Text {
    public init(_ title: LocalizedStringKey, isOn: Binding<Bool>) {
        self.init(isOn: isOn) { Text(title) }
    }
}
