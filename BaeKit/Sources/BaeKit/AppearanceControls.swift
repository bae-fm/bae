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
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text("Accent", tableName: "Appearance", bundle: .module)
                Text(preferences.accent.title)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            HStack(spacing: 0) {
                ForEach(AccentChoice.allCases, id: \.self) { accent in
                    Button {
                        preferences.accent = accent
                    } label: {
                        Circle()
                            .fill(accent.colors.fill)
                            .frame(width: 22, height: 22)
                            .overlay {
                                Image(systemName: "checkmark")
                                    .themeIcon(.small)
                                    .foregroundStyle(accent.colors.onFill)
                                    .opacity(
                                        preferences.accent == accent ? 1 : 0
                                    )
                            }
                            .frame(
                                minWidth: ThemeSize.hitTarget,
                                minHeight: ThemeSize.hitTarget
                            )
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(accent.title)
                    .accessibilityAddTraits(
                        preferences.accent == accent ? .isSelected : []
                    )
                    .help(accent.title)
                }
            }
        }
    }
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
