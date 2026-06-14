import SwiftUI

/// A small per-library glyph for the sidebar: a colored circle bearing
/// the first letter of the library name. The color is derived from the
/// library id so the same library always gets the same color across
/// launches.
struct LibraryAvatar: View {
    let library: BridgeLibrary
    /// User-chosen color override from the sidebar Color picker. When
    /// nil, the avatar falls back to a stable hash-derived palette
    /// color so libraries are still visually distinct.
    var colorOverride: Color? = nil

    private static let palette: [Color] = [
        .blue, .green, .orange, .purple, .red, .pink, .yellow, .teal, .indigo,
        .mint, .cyan,
    ]

    private var color: Color {
        if let colorOverride {
            return colorOverride
        }
        let hash = abs(library.id.hashValue)
        return Self.palette[hash % Self.palette.count]
    }

    private var initial: String {
        let first = library.name.first.map(String.init) ?? "?"
        return first.uppercased()
    }

    var body: some View {
        ZStack {
            Circle().fill(color.opacity(0.25))
            Text(initial)
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(color)
        }
        .frame(width: 20, height: 20)
    }
}
