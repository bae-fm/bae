#if os(macOS)
    import SwiftUI

    /// A checkbox drawn as the board's: a 14-point box in the field fill with a
    /// hairline edge, filled in the accent the person chose, with the check in
    /// the colour made for that fill. The native checkbox is drawn in the
    /// Mac's accent, which bae never shows. It stays a checkbox to the
    /// keyboard (Space toggles it when it has the focus) and to VoiceOver.
    public struct AccentCheckboxStyle: ToggleStyle {
        @Environment(\.accentChoice)
        private var accent

        public init() {}

        public func makeBody(configuration: Configuration) -> some View {
            Row(configuration: configuration, accent: accent)
        }

        private struct Row: View {
            let configuration: ToggleStyleConfiguration
            let accent: AccentChoice

            @FocusState
            private var focused: Bool

            var body: some View {
                Button {
                    configuration.isOn.toggle()
                } label: {
                    HStack(spacing: ThemeSpace.compact) {
                        Box(
                            isOn: configuration.isOn,
                            focused: focused,
                            accent: accent
                        )
                        configuration.label
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .focused($focused)
                .focusEffectDisabled()
                .accessibilityRepresentation {
                    Toggle(configuration).toggleStyle(.checkbox)
                }
            }
        }

        private struct Box: View {
            let isOn: Bool
            let focused: Bool
            let accent: AccentChoice

            private static let side: CGFloat = 14
            private static let radius: CGFloat = 3

            var body: some View {
                RoundedRectangle(cornerRadius: Self.radius)
                    .fill(
                        isOn
                            ? AnyShapeStyle(accent.colors.fill)
                            : AnyShapeStyle(Theme.field)
                    )
                    .overlay {
                        if !isOn {
                            RoundedRectangle(cornerRadius: Self.radius)
                                .strokeBorder(
                                    Theme.hairlineStrong,
                                    lineWidth: 1
                                )
                        }
                    }
                    .overlay {
                        if isOn {
                            Image(systemName: "checkmark")
                                .font(.system(size: 9, weight: .heavy))
                                .foregroundStyle(accent.colors.onFill)
                        }
                    }
                    .frame(width: Self.side, height: Self.side)
                    .accentFocusRing(focused, cornerRadius: Self.radius)
            }
        }
    }

    extension ToggleStyle where Self == AccentCheckboxStyle {
        /// The checkbox in the accent the person chose.
        public static var accentCheckbox: AccentCheckboxStyle {
            AccentCheckboxStyle()
        }
    }
#endif
