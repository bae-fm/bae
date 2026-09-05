import SwiftUI

/// The same appearance controls in macOS settings and the iOS settings list.
public struct AppearanceControls: View {
    private var preferences = AppearancePreferences()

    public init() {}

    public var body: some View {
        Picker(selection: preferences.$mode) {
            ForEach(AppearanceMode.allCases, id: \.self) { mode in
                Text(mode.title).tag(mode)
            }
        } label: {
            Text("Mode", tableName: "Appearance", bundle: .module)
        }
        .pickerStyle(.segmented)

        VStack(alignment: .leading, spacing: 8) {
            Text("Accent color", tableName: "Appearance", bundle: .module)
            HStack(spacing: 0) {
                ForEach(AccentChoice.allCases, id: \.self) { accent in
                    Button {
                        preferences.accent = accent
                    } label: {
                        Circle()
                            .fill(accent.buttonColor)
                            .frame(width: 22, height: 22)
                            .overlay {
                                Image(systemName: "checkmark")
                                    .font(.system(size: 11, weight: .bold))
                                    .foregroundStyle(.white)
                                    .opacity(
                                        preferences.accent == accent ? 1 : 0
                                    )
                            }
                            .frame(maxWidth: .infinity, minHeight: 44)
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

        Picker(selection: preferences.$tone) {
            ForEach(SurfaceTone.allCases, id: \.self) { tone in
                Text(tone.title).tag(tone)
            }
        } label: {
            Text("Background tone", tableName: "Appearance", bundle: .module)
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
        }
    }
}

extension AccentChoice {
    var title: String {
        switch self {
        case .blue:
            String(localized: "Blue", table: "Appearance", bundle: .module)
        case .indigo:
            String(localized: "Indigo", table: "Appearance", bundle: .module)
        case .purple:
            String(localized: "Purple", table: "Appearance", bundle: .module)
        case .pink:
            String(localized: "Pink", table: "Appearance", bundle: .module)
        case .red:
            String(localized: "Red", table: "Appearance", bundle: .module)
        case .amber:
            String(localized: "Amber", table: "Appearance", bundle: .module)
        case .green:
            String(localized: "Green", table: "Appearance", bundle: .module)
        case .teal:
            String(localized: "Teal", table: "Appearance", bundle: .module)
        }
    }
}
