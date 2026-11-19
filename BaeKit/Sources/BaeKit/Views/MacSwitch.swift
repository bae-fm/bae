import SwiftUI

/// A switch. `title` names it to VoiceOver; the row it sits in draws the
/// words, so the switch draws none of its own.
public struct MacSwitch: View {
    private let title: LocalizedStringKey
    @Binding
    private var isOn: Bool

    public init(_ title: LocalizedStringKey, isOn: Binding<Bool>) {
        self.title = title
        self._isOn = isOn
    }

    public var body: some View {
        Toggle(title, isOn: $isOn)
            .labelsHidden()
    }
}
