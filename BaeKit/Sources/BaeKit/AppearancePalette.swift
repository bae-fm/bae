import SwiftUI

public enum SurfaceTone: String, CaseIterable, Sendable {
    case neutral, slate, plum
}

public enum AccentChoice: String, CaseIterable, Sendable {
    case blue, indigo, purple, pink, red, amber, green, teal

    public func color(in scheme: ColorScheme) -> Color {
        let colors = paletteColors
        return AppearancePalette.color(
            scheme == .dark ? colors.dark : colors.light
        )
    }

    private var paletteColors: AppearancePalette.Accent {
        guard let colors = AppearancePalette.bundled.accents[rawValue] else {
            preconditionFailure("Missing accent: \(rawValue)")
        }
        return colors
    }

    public var buttonColor: Color {
        AppearancePalette.color(
            paletteColors.fill
        )
    }
}

/// The same resource is embedded by Avalonia and Android. Each tone has
/// a complete light and dark surface palette; accent fills are chosen for
/// white button labels independently of the accent used for text and glyphs.
struct AppearancePalette: Decodable, Sendable {
    struct Surfaces: Decodable, Sendable {
        let background, surface, elevated, field, fieldHover: String
        let placeholder, well, tile: String
    }

    struct Modes: Decodable, Sendable {
        let light, dark: Surfaces
    }

    struct Accent: Decodable, Sendable {
        let light, dark, fill: String
    }

    let tones: [String: Modes]
    let accents: [String: Accent]

    static let bundled: AppearancePalette = {
        do {
            guard
                let url = Bundle.module.url(
                    forResource: "AppearancePalette",
                    withExtension: "json"
                )
            else {
                preconditionFailure(
                    "AppearancePalette.json is missing from BaeKit"
                )
            }
            let palette = try JSONDecoder()
                .decode(
                    AppearancePalette.self,
                    from: Data(contentsOf: url)
                )
            precondition(
                Set(palette.tones.keys)
                    == Set(SurfaceTone.allCases.map(\.rawValue))
            )
            precondition(
                Set(palette.accents.keys)
                    == Set(AccentChoice.allCases.map(\.rawValue))
            )
            return palette
        }
        catch {
            preconditionFailure("Invalid appearance palette: \(error)")
        }
    }()

    static func color(_ hex: String) -> Color {
        precondition(
            hex.count == 7 && hex.first == "#",
            "Invalid palette color: \(hex)"
        )
        guard let rgb = UInt32(hex.dropFirst(), radix: 16) else {
            preconditionFailure("Invalid palette color: \(hex)")
        }
        return Color(
            .sRGB,
            red: Double((rgb >> 16) & 255) / 255,
            green: Double((rgb >> 8) & 255) / 255,
            blue: Double(rgb & 255) / 255,
            opacity: 1
        )
    }
}
