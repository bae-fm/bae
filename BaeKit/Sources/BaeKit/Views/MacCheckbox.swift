import SwiftUI

/// A checkbox and its label, for an on/off choice in a row of controls. On the
/// Mac it is drawn in the accent the person chose.
public struct MacCheckbox<Label: View>: View {
    @Binding
    private var isOn: Bool
    private let label: Label

    public init(
        isOn: Binding<Bool>,
        @ViewBuilder label: () -> Label
    ) {
        self._isOn = isOn
        self.label = label()
    }

    public var body: some View {
        Toggle(isOn: $isOn) { label }
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
