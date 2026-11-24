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
            .toggleStyle(MacSwitchStyle())
    }
}

/// The design system's switch: a 32 by 18 track in the tile colour, filled
/// with the chosen accent's swatch colour while on, and a white knob that
/// slides to the end that is on. The system's switch draws its track in the
/// system accent, or grey in a window that is not key, whichever accent the
/// person chose, so bae draws its own. VoiceOver reads the system switch.
private struct MacSwitchStyle: ToggleStyle {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.isEnabled)
    private var isEnabled

    private static let width: CGFloat = 32
    private static let height: CGFloat = 18
    private static let inset: CGFloat = 2
    private static let disabledOpacity = 0.45

    func makeBody(configuration: Configuration) -> some View {
        let knob = Self.height - Self.inset * 2
        return Capsule()
            .fill(
                configuration.isOn
                    ? AnyShapeStyle(accent.fillColor)
                    : AnyShapeStyle(Theme.tile)
            )
            .overlay(
                alignment: configuration.isOn ? .trailing : .leading
            ) {
                Circle()
                    .fill(Theme.onFill)
                    .frame(width: knob, height: knob)
                    .shadow(color: Theme.shadow, radius: 1, y: 1)
                    .padding(Self.inset)
            }
            .frame(width: Self.width, height: Self.height)
            .compositingGroup()
            .opacity(isEnabled ? 1 : Self.disabledOpacity)
            .contentShape(Capsule())
            .onTapGesture {
                guard isEnabled else { return }
                configuration.isOn.toggle()
            }
            .accessibilityRepresentation {
                Toggle(configuration).toggleStyle(.switch)
            }
    }
}
