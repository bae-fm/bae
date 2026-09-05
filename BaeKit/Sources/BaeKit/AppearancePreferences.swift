import SwiftUI

public enum AppearanceMode: String, CaseIterable, Sendable {
    case system, light, dark

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

/// Device preferences shared by the Apple apps. AppStorage observes the store
/// supplied by SwiftUI's defaultAppStorage environment, including test stores.
public struct AppearancePreferences: DynamicProperty {
    @AppStorage("appearance.mode")
    public var mode: AppearanceMode = .system
    @AppStorage("appearance.accent")
    public var accent: AccentChoice = .blue
    @AppStorage("appearance.tone")
    public var tone: SurfaceTone = .neutral

    public init() {}
}
