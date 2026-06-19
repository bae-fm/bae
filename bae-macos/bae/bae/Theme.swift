import SwiftUI

#if os(iOS)
    import UIKit
#endif

enum Theme {
    #if os(iOS)
        static let background = platformColor(
            light: Color(red: 0.986, green: 0.988, blue: 0.990),
            dark: darkBackground
        )
        static let surface = platformColor(
            light: Color(red: 0.958, green: 0.965, blue: 0.970),
            dark: darkSurface
        )
        static let placeholder = platformColor(
            light: Color(red: 0.890, green: 0.905, blue: 0.916),
            dark: darkPlaceholder
        )
        static let surfaceElevated = platformColor(
            light: Color(red: 0.928, green: 0.940, blue: 0.950),
            dark: darkSurfaceElevated
        )
        static let accent = platformColor(
            light: Color(red: 0.720, green: 0.330, blue: 0.230),
            dark: darkAccent
        )
    #elseif os(macOS)
        static let background = darkBackground
        static let surface = darkSurface
        static let placeholder = darkPlaceholder
        static let surfaceElevated = darkSurfaceElevated
        static let accent = darkAccent
    #endif

    #if os(macOS)
        static let field = Color(red: 0.063, green: 0.063, blue: 0.098)
        static let fieldHover = Color(red: 0.078, green: 0.078, blue: 0.122)
    #endif

    static let accentSoft =
        accent
        .opacity(0.22)

    private static let darkBackground = Color(
        red: 0.060,
        green: 0.060,
        blue: 0.090
    )
    private static let darkSurface = Color(
        red: 0.090,
        green: 0.090,
        blue: 0.130
    )
    private static let darkPlaceholder = Color(
        red: 0.120,
        green: 0.120,
        blue: 0.170
    )
    private static let darkSurfaceElevated = Color(
        red: 0.110,
        green: 0.110,
        blue: 0.157
    )
    private static let darkAccent = Color(red: 0.850, green: 0.470, blue: 0.340)

    #if os(iOS)
        private static func platformColor(light: Color, dark: Color) -> Color {
            Color(
                UIColor { traits in
                    UIColor(traits.userInterfaceStyle == .dark ? dark : light)
                }
            )
        }
    #endif
}

extension View {
    /// The app window's base appearance: the dark background every screen sits
    /// on, plus the pinned dark color scheme. Declared once here and applied at
    /// the app root and in previews, so the two can't drift apart.
    func windowBackground() -> some View {
        self
            .background(Theme.background)
            .preferredColorScheme(.dark)
    }
}

#Preview("Theme Test") {
    Text("Hello")
        .padding()
        .background(Theme.background)
}
