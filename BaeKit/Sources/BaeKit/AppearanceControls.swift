import SwiftUI

/// The appearance controls in settings.
public struct AppearanceControls: View {
    private var preferences = AppearancePreferences()

    public init() {}

    public var body: some View {
        Picker(selection: preferences.$mode) {
            ForEach(AppearanceMode.allCases, id: \.self) { mode in
                Text(mode.title).tag(mode)
            }
        } label: {
            Text("Theme", tableName: "Appearance", bundle: .module)
        }

        Picker(selection: preferences.$tone) {
            ForEach(SurfaceTone.allCases, id: \.self) { tone in
                Text(tone.title).tag(tone)
            }
        } label: {
            Text("Tone", tableName: "Appearance", bundle: .module)
        }

        HStack(alignment: .center, spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text("Accent", tableName: "Appearance", bundle: .module)
                Text(preferences.accent.title)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            MacAccentPicker(
                selected: preferences.accent,
                onSelect: { preferences.accent = $0 }
            )
        }
    }
}

/// The accent swatches at the end of the Accent row: a disc of each colour,
/// the chosen one ringed.
public struct MacAccentPicker: View {
    let selected: AccentChoice
    let onSelect: (AccentChoice) -> Void

    public init(
        selected: AccentChoice,
        onSelect: @escaping (AccentChoice) -> Void
    ) {
        self.selected = selected
        self.onSelect = onSelect
    }

    public var body: some View {
        HStack(spacing: ThemeSpace.related) {
            ForEach(AccentChoice.allCases, id: \.self) { accent in
                swatch(accent)
            }
        }
    }

    /// One accent's disc, ringed while it is the chosen one.
    private func swatch(_ accent: AccentChoice) -> some View {
        let chosen = selected == accent
        return Button {
            onSelect(accent)
        } label: {
            Circle()
                .fill(accent.colors.fill)
                .frame(width: Self.swatchSize, height: Self.swatchSize)
                .overlay {
                    if chosen {
                        // A gap of the surface, then the ring.
                        Circle()
                            .stroke(Color.primary, lineWidth: Self.ringWidth)
                            .frame(
                                width: Self.swatchSize + 2 * Self.ringOffset,
                                height: Self.swatchSize + 2 * Self.ringOffset
                            )
                    }
                }
                // The press target runs past the disc, up to its neighbours.
                .contentShape(Circle().inset(by: -ThemeSpace.compact / 2))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accent.title)
        .accessibilityAddTraits(chosen ? .isSelected : [])
        .help(accent.title)
    }

    private static let swatchSize: CGFloat = 18
    private static let ringWidth: CGFloat = 1.5
    /// From a disc's edge to the middle of its ring: the 2 pt gap and half the
    /// ring's own width.
    private static let ringOffset: CGFloat = 2 + ringWidth / 2
}

extension AppearanceMode {
    var title: String {
        switch self {
        case .system:
            String(localized: "System", table: "Appearance", bundle: .module)
        case .light:
            String(localized: "Light", table: "Appearance", bundle: .module)
        case .dark:
            String(localized: "Dark", table: "Appearance", bundle: .module)
        }
    }
}

extension SurfaceTone {
    var title: String {
        switch self {
        case .neutral:
            String(localized: "Neutral", table: "Appearance", bundle: .module)
        case .slate:
            String(localized: "Slate", table: "Appearance", bundle: .module)
        case .plum:
            String(localized: "Plum", table: "Appearance", bundle: .module)
        case .midnight:
            String(localized: "Midnight", table: "Appearance", bundle: .module)
        case .forest:
            String(localized: "Forest", table: "Appearance", bundle: .module)
        case .sand:
            String(localized: "Sand", table: "Appearance", bundle: .module)
        }
    }
}

extension AccentChoice {
    var title: String {
        switch self {
        case .blue:
            String(localized: "Blue", table: "Appearance", bundle: .module)
        case .orchid:
            String(localized: "Orchid", table: "Appearance", bundle: .module)
        case .pink:
            String(localized: "Pink", table: "Appearance", bundle: .module)
        case .coral:
            String(localized: "Coral", table: "Appearance", bundle: .module)
        case .gold:
            String(localized: "Gold", table: "Appearance", bundle: .module)
        case .lime:
            String(localized: "Lime", table: "Appearance", bundle: .module)
        case .mint:
            String(localized: "Mint", table: "Appearance", bundle: .module)
        case .sky:
            String(localized: "Sky", table: "Appearance", bundle: .module)
        }
    }
}
